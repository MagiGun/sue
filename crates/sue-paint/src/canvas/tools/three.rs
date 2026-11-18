//! The Operation tool on 3D layers: a drag turns the camera (Shift moves
//! it, Ctrl comes nearer or goes further), and on a drawing figure a body
//! part bends at its joint (Shift twists it).

use egui::Pos2;
use sue_wm::AppletContext;

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};

/// How a drawing figure is seen on its 3D layer.
pub(crate) struct FigureSight {
    /// Its model.
    pub(crate) model: sue_core::EntityId,
    /// Its pose.
    pub(crate) pose: crate::figure::Pose,
    /// Its body.
    pub(crate) body: crate::figure::Body,
    /// The layer's camera.
    pub(crate) lens: crate::three::Camera,
    /// Where the layer sits on the picture.
    pub(crate) offset: [f32; 2],
}

impl SuePaint {
    /// The Operation tool pressed on a 3D layer.
    pub(super) fn three_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            modifiers,
            size,
            ..
        } = *press;
        // A 3D layer: drag to turn the camera, Shift to move it,
        // Ctrl to come nearer or go further.
        let Some(layer) = self.layer() else {
            return;
        };
        // A drawing figure: a ring of the picked joint's rotation
        // manipulator turns it about one axis; a body part under the
        // pointer goes where it is dragged (a hand or foot with its arm
        // or leg following; Shift twists the part instead).
        let _ = size;
        if let Some(sight) = self.figure_sight(&cx.world.arena, layer) {
            let local = [p.x - sight.offset[0], p.y - sight.offset[1]];
            let zoom = self.doc().map_or(1.0, |doc| doc.view.zoom).max(0.01);
            let reach = 7.0 / zoom;
            let ring = self.figure_joint.and_then(|joint| {
                let rings = crate::figure::rings(&sight.pose, &sight.body, joint)?;
                let center = sight
                    .lens
                    .project(crate::figure::solve(&sight.pose, &sight.body).get(joint)?.0)?;
                let mut best: Option<(usize, f32, f32)> = None;
                for (axis, ring) in rings.iter().enumerate() {
                    let flat: Vec<[f32; 2]> =
                        ring.iter().filter_map(|p| sight.lens.project(*p)).collect();
                    let near = flat
                        .windows(2)
                        .map(|pair| {
                            crate::canvas::overlays::segment_distance(local, pair[0], pair[1])
                        })
                        .fold(f32::INFINITY, f32::min);
                    // Which way round the ring runs on the layer.
                    let round: f32 = flat
                        .windows(2)
                        .map(|pair| {
                            (pair[0][0] - center[0]) * (pair[1][1] - center[1])
                                - (pair[0][1] - center[1]) * (pair[1][0] - center[0])
                        })
                        .sum();
                    if near <= reach && best.is_none_or(|(_, least, _)| near < least) {
                        best = Some((axis, near, round.signum()));
                    }
                }
                best.map(|(axis, _, way)| (joint, axis, center, way))
            });
            // An arrow of the translate manipulator at the root: it
            // moves the whole body along one of the world's axes.
            let arrow = if ring.is_some() {
                None
            } else {
                self.figure_joint.and_then(|joint| {
                    let arrows = crate::figure::arrows(&sight.pose, &sight.body, joint)?;
                    let mut best: Option<(usize, f32, [f32; 3])> = None;
                    for (axis, [from, to]) in arrows.iter().enumerate() {
                        let (Some(a), Some(b)) =
                            (sight.lens.project(*from), sight.lens.project(*to))
                        else {
                            continue;
                        };
                        let near = crate::canvas::overlays::segment_distance(local, a, b);
                        if near <= reach && best.is_none_or(|(_, least, _)| near < least) {
                            best = Some((axis, near, *from));
                        }
                    }
                    best.map(|(axis, _, from)| (joint, axis, from))
                })
            };
            let part = if ring.is_some() || arrow.is_some() {
                None
            } else {
                self.figure_part_at(&cx.world.arena, layer, p, size)
            };
            let before = doc::raster(&cx.world.arena, layer)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec);
            let stood = cx
                .world
                .arena
                .get_by_id(sight.model)
                .map_or([None; 3], |model| {
                    [
                        crate::three::OBJECT_X,
                        crate::three::OBJECT_Y,
                        crate::three::OBJECT_Z,
                    ]
                    .map(|key| {
                        model
                            .property(key)
                            .and_then(sue_core::Value::as_f64)
                            .map(|value| value as f32)
                    })
                });
            if let (Some((joint, axis, center, way)), Some(before)) = (ring, before.clone()) {
                self.gesture = Some(Gesture::Pose {
                    layer,
                    joint,
                    start: pos,
                    base: sight.pose,
                    before,
                    how: 3 + axis as u8,
                    stood,
                    grip: local,
                    like: [center[0], center[1], way],
                });
                return;
            }
            if let (Some((joint, axis, from)), Some(before)) = (arrow, before.clone()) {
                self.gesture = Some(Gesture::Pose {
                    layer,
                    joint,
                    start: pos,
                    base: sight.pose,
                    before,
                    how: 6 + axis as u8,
                    stood,
                    grip: local,
                    like: from,
                });
                return;
            }
            if let (Some(joint), Some(before)) = (part, before) {
                let joints = sight.body.joints();
                let placed = crate::figure::bones(&sight.pose, &sight.body);
                // What the drag carries: the wrist or ankle of a limb,
                // else the far end of the part.
                let limb = crate::figure::limb_of(&sight.body, joint);
                let root = joints
                    .get(joint)
                    .is_some_and(|joint| joint.parent.is_none());
                let trunk = sight.body.kind == crate::figure::Kind::Body && matches!(joint, 1 | 2);
                let like = match limb {
                    Some(limb) => placed[limb[2]].0,
                    // The hips are carried by their own place, the
                    // trunk by the foot of the neck.
                    None if root => placed[joint].0,
                    None if trunk => placed[3].0,
                    None => placed[joint].1,
                };
                self.figure_joint = Some(joint);
                self.gesture = Some(Gesture::Pose {
                    layer,
                    joint,
                    start: pos,
                    base: sight.pose,
                    before,
                    // The hips carry the whole body where they are
                    // dragged (Ctrl bends and swings it instead); the
                    // spine and chest lean the trunk.
                    how: if modifiers.shift {
                        1
                    } else if root && modifiers.command {
                        0
                    } else if root {
                        9
                    } else if sight.body.kind == crate::figure::Kind::Body && matches!(joint, 1 | 2)
                    {
                        10
                    } else {
                        2
                    },
                    stood,
                    grip: local,
                    like,
                });
                return;
            }
        }
        let (Some(base), Some(before)) = (
            cx.world
                .arena
                .get_by_id(layer)
                .ok()
                .map(crate::three::View3D::of),
            doc::raster(&cx.world.arena, layer)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec),
        ) else {
            return;
        };
        let mode = if modifiers.shift {
            1
        } else if modifiers.command {
            2
        } else {
            0
        };
        self.gesture = Some(Gesture::Orbit {
            layer,
            start: pos,
            base,
            before,
            mode,
        });
    }

    /// A drawing figure posed by a drag to `pos`: its picked part taken
    /// where the pointer goes (a hand or foot with its limb following,
    /// pinned hands and feet staying put), twisted, or turned about one
    /// axis by a ring of the rotation manipulator.
    pub(super) fn pose_drag(
        &mut self,
        pos: Pos2,
        viewport: egui::Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::Pose {
            layer,
            joint,
            start,
            base,
            how,
            stood,
            grip,
            like,
            ..
        }) = &self.gesture
        else {
            return;
        };
        let (layer, joint, how, grip, like) = (*layer, *joint, *how, *grip, *like);
        let stood = *stood;
        let (dx, dy) = (pos.x - start.x, pos.y - start.y);
        let base = base.clone();
        let mut pose = base.clone();
        if pose.0.len() <= joint {
            return;
        }
        let Some(sight) = self.figure_sight(&cx.world.arena, layer) else {
            return;
        };
        // The body as it stood when the drag began: the previews since
        // have moved it.
        let mut body = sight.body;
        body.place.position = stood.map(|value| value.unwrap_or(0.0));
        let keep = self.figure_limits;
        let pinned: Vec<usize> = cx
            .world
            .arena
            .get_by_id(sight.model)
            .map(crate::figure::pins_of)
            .unwrap_or_default();
        let mut stands = None;
        let here = self
            .doc()
            .map(|doc| doc.view.to_picture(viewport, size, pos));
        let local = here.map(|p| [p.x - sight.offset[0], p.y - sight.offset[1]]);
        // A press that has not moved yet moves nothing.
        if matches!(how, 6..=10) && local.is_some_and(|local| local == grip) {
            return;
        }
        match (how, local) {
            (1, _) => {
                pose.0[joint][1] = base.0[joint][1] + dx * 0.6;
                if keep {
                    pose = crate::figure::limited(&pose, &body);
                }
            }
            (2, Some(local)) => {
                // Where the carried point is taken: as far across the
                // layer as the pointer went, as far from the camera as
                // it was.
                let Some(from) = sight.lens.project(like) else {
                    return;
                };
                let to = [from[0] + local[0] - grip[0], from[1] + local[1] - grip[1]];
                let target = sight.lens.unproject(to, like);
                pose = match crate::figure::limb_of(&body, joint) {
                    Some(limb) => crate::figure::reached(&base, &body, limb, target, keep),
                    None => crate::figure::aimed_at(&base, &body, joint, target, keep),
                };
            }
            (6..=9, Some(local)) => {
                // The whole body: along an arrow's axis as far as the
                // pointer went along the arrow, or where the pointer
                // takes the hips, as far from the camera as they were.
                let lens = crate::three::Camera::new(
                    &cx.world
                        .arena
                        .get_by_id(layer)
                        .ok()
                        .map(crate::three::View3D::of)
                        .unwrap_or_default(),
                    self.figure_canvas(&cx.world.arena, layer),
                    body.frame(),
                );
                let Some(from) = lens.project(like) else {
                    return;
                };
                let by = [local[0] - grip[0], local[1] - grip[1]];
                let shift = if how == 9 {
                    let to = lens.unproject([from[0] + by[0], from[1] + by[1]], like);
                    [0, 1, 2].map(|axis| to[axis] - like[axis])
                } else {
                    let axis = usize::from(how - 6);
                    let mut ahead = like;
                    ahead[axis] += 1.0;
                    let Some(to) = lens.project(ahead) else {
                        return;
                    };
                    let along = [to[0] - from[0], to[1] - from[1]];
                    let long = along[0] * along[0] + along[1] * along[1];
                    let mut shift = [0.0; 3];
                    if long > 1e-6 {
                        shift[axis] = (by[0] * along[0] + by[1] * along[1]) / long;
                    }
                    shift
                };
                let (moved, position) = crate::figure::moved(&base, &body, shift, &pinned, keep);
                pose = moved;
                stands = Some(position);
            }
            (10, Some(local)) => {
                let Some(from) = sight.lens.project(like) else {
                    return;
                };
                let to = [from[0] + local[0] - grip[0], from[1] + local[1] - grip[1]];
                let target = sight.lens.unproject(to, like);
                pose = crate::figure::leaned(&base, &body, joint, target, keep);
            }
            (3..=5, Some(local)) => {
                let angle = |p: [f32; 2]| (p[1] - like[1]).atan2(p[0] - like[0]).to_degrees();
                let mut by = angle(local) - angle(grip);
                if by > 180.0 {
                    by -= 360.0;
                } else if by < -180.0 {
                    by += 360.0;
                }
                pose.0[joint] =
                    crate::figure::turned(base.0[joint], usize::from(how - 3), by * like[2]);
                if keep {
                    pose = crate::figure::limited(&pose, &body);
                }
            }
            _ => {
                // The whole figure (its root): bent and swung.
                let behind = cx
                    .world
                    .arena
                    .get_by_id(layer)
                    .ok()
                    .map(crate::three::View3D::of)
                    .is_some_and(|view| view.yaw.to_radians().cos() < 0.0);
                let side = if behind { -1.0 } else { 1.0 };
                let turn = &mut pose.0[joint];
                turn[0] = (base.0[joint][0] + dy * 0.5).clamp(-180.0, 180.0);
                turn[2] = (base.0[joint][2] + dx * 0.5 * side).clamp(-180.0, 180.0);
            }
        }
        // Pinned hands and feet stay where they were (but not the one
        // being dragged).
        let dragged = crate::figure::limb_of(&body, joint).map(|limb| limb[2]);
        let pins: Vec<usize> = pinned
            .into_iter()
            .filter(|pin| Some(*pin) != dragged || how != 2)
            .collect();
        // (Moving the whole body has held them already.)
        if !pins.is_empty() && stands.is_none() {
            pose = crate::figure::held(&pose, &base, &body, &pins, keep);
        }
        self.preview_pose(cx, layer, &pose, stands);
    }

    /// The size of 3D layer `layer`'s picture.
    fn figure_canvas(&self, arena: &sue_core::Arena, layer: sue_core::EntityId) -> [u32; 2] {
        doc::raster(arena, layer).map_or([1, 1], |raster| [raster.width(), raster.height()])
    }

    /// How the drawing figure of 3D layer `layer` is seen: its camera,
    /// where the layer sits on the picture, and its model, pose and body.
    pub(crate) fn figure_sight(
        &self,
        arena: &sue_core::Arena,
        layer: sue_core::EntityId,
    ) -> Option<FigureSight> {
        let model = crate::figure::of_layer(arena, layer)?;
        let entity = arena.get_by_id(model).ok()?;
        let (pose, body) = (
            crate::figure::Pose::of(entity),
            crate::figure::Body::of(entity),
        );
        let view = crate::three::View3D::of(arena.get_by_id(layer).ok()?);
        let raster = doc::raster(arena, layer)?;
        let lens =
            crate::three::Camera::new(&view, [raster.width(), raster.height()], body.frame());
        let offset = self
            .doc()
            .and_then(|doc| {
                doc::layers(arena, doc.picture)
                    .into_iter()
                    .find(|info| info.id == layer)
            })
            .map_or([0.0; 2], |info| {
                [info.style.offset[0] as f32, info.style.offset[1] as f32]
            });
        Some(FigureSight {
            model,
            pose,
            body,
            lens,
            offset,
        })
    }

    /// The 3D camera turned, moved or brought nearer by a drag to `pos`.
    pub(super) fn orbit_drag(&mut self, pos: Pos2, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Orbit {
            layer,
            start,
            base,
            mode,
            ..
        }) = &self.gesture
        else {
            return;
        };
        let (layer, start, base, mode) = (*layer, *start, *base, *mode);
        let (dx, dy) = (pos.x - start.x, pos.y - start.y);
        let mut view = base;
        match mode {
            1 => {
                let scale = base.distance / 600.0;
                view.pan = [base.pan[0] - dx * scale, base.pan[1] + dy * scale];
            }
            2 => view.distance = (base.distance * (dy / 200.0).exp()).clamp(0.6, 100.0),
            _ => {
                view.yaw = base.yaw - dx * 0.4;
                view.pitch = (base.pitch + dy * 0.4).clamp(-89.0, 89.0);
            }
        }
        self.preview_view3d(cx, layer, &view);
    }
}
