//! The handles the canvas drags that are no tool's stroke: the layers of
//! the light table, the pins of the puppet warp, the camera frame and the
//! transform box.

use egui::{Modifiers, Pos2, Rect, pos2};
use sue_wm::AppletContext;

use super::Press;
use crate::studio::{Gesture, SuePaint};
use crate::view::View;

impl SuePaint {
    /// The layers the Light table tool moves (the light table of the cel
    /// being drawn, and the general one while it is on), each with its
    /// place, and the box around what they show: the frame its handles
    /// are on (canvas pixels). `None` with no layer on a light table.
    pub(crate) fn light_table_box(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<(
        Vec<(sue_core::EntityId, crate::anim::Place)>,
        crate::transform::Quad,
    )> {
        let mut layers = self
            .editing_cel(arena)
            .map(|cel| crate::anim::light_table(arena, cel))
            .unwrap_or_default();
        if self.light_general
            && let Some(picture) = self.picture()
        {
            layers.extend(crate::anim::general_light_table(arena, picture));
        }
        let placed: Vec<(sue_core::EntityId, crate::anim::Place)> = layers
            .into_iter()
            .filter_map(|layer| Some((layer, crate::anim::Place::of(arena.get_by_id(layer).ok()?))))
            .collect();
        let (_, first) = placed.first()?;
        let size = self
            .picture()
            .and_then(|picture| arena.get_by_id(picture).ok())
            .and_then(sue_model::picture::size)?;
        // What the layers show, where they are on their own.
        let mut bounds: Option<[f32; 4]> = None;
        for (layer, _) in &placed {
            let Some(raster) = crate::doc::raster(arena, *layer) else {
                continue;
            };
            let Some(bytes) = raster.bytes() else {
                continue;
            };
            let Some([x0, y0, x1, y1]) =
                crate::warp::bounds(bytes, raster.width(), raster.height())
            else {
                continue;
            };
            let offset = crate::doc::layers(arena, self.picture()?)
                .into_iter()
                .find(|info| info.id == *layer)
                .map_or([0, 0], |info| info.style.offset)
                .map(|value| value as f32);
            let found = [
                x0 as f32 + offset[0],
                y0 as f32 + offset[1],
                x1 as f32 + offset[0],
                y1 as f32 + offset[1],
            ];
            bounds = Some(bounds.map_or(found, |b| {
                [
                    b[0].min(found[0]),
                    b[1].min(found[1]),
                    b[2].max(found[2]),
                    b[3].max(found[3]),
                ]
            }));
        }
        let [x0, y0, x1, y1] = bounds.unwrap_or([0.0, 0.0, size[0] as f32, size[1] as f32]);
        let quad = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|at| first.forward(at, size));
        Some((placed, quad))
    }

    /// The current layer when the Object tool keys it (keyframes are on
    /// for it): the layer, how it stands at the frame shown, the box round
    /// its drawing where it shows (picture pixels), and the canvas size.
    pub(crate) fn keyed_box(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<(
        sue_core::EntityId,
        crate::anim::motion::Pose,
        crate::transform::Quad,
        [u32; 2],
    )> {
        if self.toolbox.tool() != crate::tools::Tool::Operation || self.light_tool {
            return None;
        }
        let layer = self.layer()?;
        let entity = arena.get_by_id(layer).ok()?;
        if !crate::anim::motion::keyframes_on(entity) || crate::camera::is_camera(arena, layer) {
            return None;
        }
        let picture = self.picture()?;
        let size = arena
            .get_by_id(picture)
            .ok()
            .and_then(sue_model::picture::size)?;
        let (_, time) = self.frame_time(arena);
        let pose = crate::anim::motion::pose(arena, layer, time, size)
            .unwrap_or_else(|| crate::anim::motion::Pose::still(size));
        let offset = crate::doc::layers(arena, picture)
            .into_iter()
            .find(|info| info.id == layer)
            .map_or([0, 0], |info| info.style.offset)
            .map(|value| value as f32);
        let [x0, y0, x1, y1] = crate::doc::raster(arena, layer)
            .and_then(|raster| {
                crate::warp::bounds(raster.bytes()?, raster.width(), raster.height())
            })
            .map_or(
                [0.0, 0.0, size[0] as f32, size[1] as f32],
                |[x0, y0, x1, y1]| {
                    [
                        x0 as f32 + offset[0],
                        y0 as f32 + offset[1],
                        x1 as f32 + offset[0],
                        y1 as f32 + offset[1],
                    ]
                },
            );
        let quad = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|at| pose_forward(&pose, at));
        Some((layer, pose, quad, size))
    }

    /// The Object tool pressed on a layer whose keyframes are on: the box
    /// round its drawing moves it (inside), scales it (a corner or an
    /// edge) or turns it (the stalk, or outside), keyed at the frame shown
    /// when let go.
    pub(super) fn keyed_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            viewport,
            size,
            ..
        } = *press;
        let Some((layer, pose, quad, _)) = self.keyed_box(&cx.world.arena) else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let shown = quad.map(|at| view.to_screen(viewport, size, pos2(at[0], at[1])));
        let (_, stalk) = crate::transform_ui::stalk_of(&shown, crate::canvas::marks::STALK);
        let grip = crate::transform_ui::frame_grip(&shown, stalk, pos, true);
        self.gesture = Some(Gesture::Keyed {
            layer,
            start: [p.x, p.y],
            base: pose,
            grip,
            quad,
            preview: None,
        });
    }

    /// The keyed layer dragged to `pos`: shown as it goes (Alt scales
    /// about the middle, Shift keeps a move to one axis and turns by 15
    /// degrees), keyed when let go.
    pub(super) fn keyed_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        use crate::transform_ui::{FrameGrip, constrained, grip_points, scale_ratio};
        let (alt, shift) = (self.modifiers.alt, self.modifiers.shift);
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let (_, time) = self.frame_time(&cx.world.arena);
        let p = view.to_picture(viewport, size, pos);
        let Some(Gesture::Keyed {
            layer,
            start,
            base,
            grip,
            quad,
            preview,
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let middle = crate::transform::middle(quad);
        let mut pose = *base;
        // A similarity about `about` (turned `degrees`, scaled `by`) laid
        // on how the layer stood.
        let about =
            |pose: &mut crate::anim::motion::Pose, about: [f32; 2], degrees: f32, by: f32| {
                let (sin, cos) = degrees.to_radians().sin_cos();
                let c = pose.center;
                let v = [
                    c[0] + pose.position[0] - about[0],
                    c[1] + pose.position[1] - about[1],
                ];
                let turned = [
                    by * (cos * v[0] - sin * v[1]),
                    by * (sin * v[0] + cos * v[1]),
                ];
                pose.position = [about[0] + turned[0] - c[0], about[1] + turned[1] - c[1]];
                pose.rotation += degrees;
                pose.scale = (pose.scale * by).clamp(0.01, 100.0);
            };
        match *grip {
            FrameGrip::Inside | FrameGrip::Centre => {
                let by = constrained([p.x - start[0], p.y - start[1]], shift);
                pose.position = [base.position[0] + by[0], base.position[1] + by[1]];
            }
            FrameGrip::Turn => {
                let step = if shift { 15_f32.to_radians() } else { 0.0 };
                let by = crate::transform::drag_angle(middle, *start, [p.x, p.y], step);
                about(&mut pose, middle, by.to_degrees(), 1.0);
            }
            FrameGrip::Corner(_) | FrameGrip::Edge(_) => {
                if let Some((held, anchor)) = grip_points(quad, *grip, alt) {
                    let k = scale_ratio(anchor, held, [p.x, p.y]);
                    about(&mut pose, anchor, 0.0, k);
                }
            }
        }
        let arena = &mut cx.world.arena;
        if let Some(mut shown) = preview.take() {
            let _ = sue_core::ReversibleTransaction::revert(&mut *shown, arena);
        }
        let ease = crate::anim::motion::ease_at(arena, *layer, time)
            .unwrap_or(crate::anim::motion::Ease::Linear);
        let mut edit = crate::anim::motion::key_pose(arena, *layer, time, &pose, ease);
        if sue_core::ReversibleTransaction::apply(&mut edit, arena).is_ok() {
            *preview = Some(Box::new(edit));
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// The keyed layer let go: its key at the frame shown, one undo step.
    pub(super) fn keyed_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::Keyed {
            preview: Some(mut shown),
            ..
        } = gesture
        else {
            return;
        };
        // Back as it was, then the key as one step.
        if sue_core::ReversibleTransaction::revert(&mut *shown, &mut cx.world.arena).is_ok() {
            Self::apply(cx, *shown);
        }
    }

    /// The Light table tool pressed at `p` (picture pixels): a corner or
    /// an edge of its box scales the light table's layers, the handle on
    /// the stalk turns them about the centre of rotation, Alt and a drag
    /// inside puts that centre where the drag goes, anywhere else moves
    /// them (Shift and a press turns them, as 0.0.75 did). The layers themselves stay as they are. `true` when
    /// there are layers to move.
    pub(in crate::canvas) fn light_table_press(
        &mut self,
        p: Pos2,
        modifiers: Modifiers,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        use crate::transform_ui::{FrameGrip, frame_grip, stalk_of};
        let Some((layers, quad)) = self.light_table_box(&cx.world.arena) else {
            return false;
        };
        let (Some(view), Some(size)) = (
            self.docs.get(self.active).map(|doc| doc.view),
            self.picture()
                .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                .and_then(sue_model::picture::size),
        ) else {
            return false;
        };
        let viewport = self.viewport;
        let shown = quad.map(|at| view.to_screen(viewport, size, pos2(at[0], at[1])));
        let (_, stalk) = stalk_of(&shown, crate::canvas::marks::STALK);
        let pos = view.to_screen(viewport, size, p);
        // A plain press inside moves the layers, as 0.0.75 did, even on
        // the centre's mark; Alt and a drag puts the centre where it goes.
        let grip = match frame_grip(&shown, stalk, pos, false) {
            FrameGrip::Inside if modifiers.alt => FrameGrip::Centre,
            FrameGrip::Inside if modifiers.shift => FrameGrip::Turn,
            grip => grip,
        };
        let layers = Self::with_kept(&cx.world.arena, layers);
        self.gesture = Some(Gesture::LightTable {
            layers,
            start: p,
            grip,
            quad,
        });
        true
    }

    /// The light table's layers dragged to `pos`: shown as they move, one
    /// undo step when let go. Alt scales about the box's middle instead of
    /// its other side; Shift keeps a move along one axis and turns in
    /// steps of 15 degrees.
    pub(super) fn light_table_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        use crate::transform_ui::{FrameGrip, constrained, grip_points, scale_ratio};
        let (alt, shift) = (self.modifiers.alt, self.modifiers.shift);
        let (
            Some(Gesture::LightTable {
                layers,
                start,
                grip,
                quad,
            }),
            Some(doc),
        ) = (self.gesture.as_ref(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let size = doc.compositor.display().1;
        let middle = crate::transform::middle(quad);
        if *grip == FrameGrip::Centre {
            // The centre goes where the pen is; the layers stay.
            self.light_centre = [p.x - middle[0], p.y - middle[1]];
            return;
        }
        let about = [
            middle[0] + self.light_centre[0],
            middle[1] + self.light_centre[1],
        ];
        let change = |place: crate::anim::Place| -> crate::anim::Place {
            match *grip {
                FrameGrip::Inside | FrameGrip::Centre => {
                    let by = constrained([p.x - start.x, p.y - start.y], shift);
                    crate::anim::Place {
                        dx: place.dx + by[0],
                        dy: place.dy + by[1],
                        ..place
                    }
                }
                FrameGrip::Turn => {
                    let step = if shift { 15_f32.to_radians() } else { 0.0 };
                    let by =
                        crate::transform::drag_angle(about, [start.x, start.y], [p.x, p.y], step);
                    place.then(about, by.to_degrees(), 1.0, false, size)
                }
                FrameGrip::Corner(_) | FrameGrip::Edge(_) => {
                    let Some((held, anchor)) = grip_points(quad, *grip, alt) else {
                        return place;
                    };
                    let k = scale_ratio(anchor, held, [p.x, p.y]);
                    place.then(anchor, 0.0, k, false, size)
                }
            }
        };
        for (layer, was, _) in layers {
            let place = change(*was);
            if let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) {
                entity
                    .properties_mut()
                    .set(crate::anim::PLACE, place.text());
            }
        }
        doc.compositor
            .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
    }

    /// The light table's layers let go.
    pub(super) fn light_table_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        use crate::transform_ui::FrameGrip;
        let Gesture::LightTable {
            layers, grip, quad, ..
        } = gesture
        else {
            return;
        };
        // The centre of rotation: a turn keeps it where it is on the
        // canvas (the box turned about it), a scale keeps its place in the
        // box, a move takes it along.
        let was = crate::transform::middle(&quad);
        let about = [was[0] + self.light_centre[0], was[1] + self.light_centre[1]];
        match grip {
            FrameGrip::Turn => {
                if let Some((_, now)) = self.light_table_box(&cx.world.arena) {
                    let now = crate::transform::middle(&now);
                    self.light_centre = [about[0] - now[0], about[1] - now[1]];
                }
            }
            FrameGrip::Corner(_) | FrameGrip::Edge(_) => {
                if let Some((layer, before, _)) = layers.first()
                    && let Ok(entity) = cx.world.arena.get_by_id(*layer)
                {
                    let ratio = crate::anim::Place::of(entity).scale / before.scale.max(0.001);
                    self.light_centre = self.light_centre.map(|value| value * ratio);
                }
            }
            FrameGrip::Inside | FrameGrip::Centre => {}
        }
        Self::light_table_step(cx, &layers);
    }

    /// `layers` with the value each keeps its place in now.
    pub(crate) fn with_kept(
        arena: &sue_core::Arena,
        layers: Vec<(sue_core::EntityId, crate::anim::Place)>,
    ) -> Vec<(
        sue_core::EntityId,
        crate::anim::Place,
        Option<sue_core::Value>,
    )> {
        layers
            .into_iter()
            .map(|(layer, place)| {
                let kept = arena
                    .get_by_id(layer)
                    .ok()
                    .and_then(|entity| entity.property(crate::anim::PLACE).cloned());
                (layer, place, kept)
            })
            .collect()
    }

    /// The light table's layers, placed in place already, as one undo
    /// step from where they were (`layers`: each one's place then and the
    /// value it was kept as).
    pub(crate) fn light_table_step(
        cx: &mut AppletContext<'_>,
        layers: &[(
            sue_core::EntityId,
            crate::anim::Place,
            Option<sue_core::Value>,
        )],
    ) {
        // Back to where they began, exactly, then the move as one step.
        let mut edit = crate::doc::Edit::new(tr!("light-table-tool"));
        for (layer, was, kept) in layers {
            let Ok(entity) = cx.world.arena.get_by_id_mut(*layer) else {
                continue;
            };
            let now = crate::anim::Place::of(entity);
            match kept {
                Some(value) => {
                    entity
                        .properties_mut()
                        .set(crate::anim::PLACE, value.clone());
                }
                None => {
                    entity.properties_mut().remove(crate::anim::PLACE);
                }
            }
            if now != *was {
                edit = edit.then(sue_core::txn::SetProperty::new(
                    *layer,
                    crate::anim::PLACE,
                    now.text(),
                ));
            }
        }
        if !edit.is_empty() {
            Self::apply(cx, edit);
        }
    }

    /// Puppet warp pressed at `pos` (on screen): a pin under the pointer
    /// is picked (Shift adds it to the picked ones) and dragged with them,
    /// Alt takes it away; a picked pin's ring turns the picked pins;
    /// anywhere else a new pin goes in, or with Click to add pins off (or
    /// Shift) a box picks the pins in it.
    pub(super) fn warp_press(
        &mut self,
        pos: Pos2,
        view: View,
        modifiers: Modifiers,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        use crate::canvas::puppet::{PinDrag, PinHow, RING, picked_middle};
        let settings = self.transform_prefs.puppet;
        let p = view.to_picture(viewport, size, pos);
        let Some(warp) = self.warp.as_mut() else {
            return;
        };
        let offset = warp.offset.map(|value| value as f32);
        let local = [p.x - offset[0], p.y - offset[1]];
        let on_screen = |at: [f32; 2]| {
            view.to_screen(viewport, size, pos2(at[0] + offset[0], at[1] + offset[1]))
        };
        let reach = crate::canvas::marks::GRAB_REACH;
        let hit = warp
            .pins
            .iter()
            .position(|pin| on_screen(pin.to).distance(pos) <= reach);
        let ring = warp
            .pins
            .iter()
            .any(|pin| pin.picked && (on_screen(pin.to).distance(pos) - RING).abs() <= reach / 2.0);
        let how = match hit {
            Some(index) if modifiers.alt => {
                warp.pins.remove(index);
                self.warp_preview(cx);
                return;
            }
            Some(index) => {
                if modifiers.shift && settings.select_pins {
                    crate::warp::pick(&mut warp.pins, index, true);
                } else if !warp.pins[index].picked || !settings.select_pins {
                    crate::warp::pick(&mut warp.pins, index, false);
                }
                PinHow::Move
            }
            None if ring => {
                let about = picked_middle(&warp.pins).unwrap_or(local);
                PinHow::Turn { about }
            }
            None if settings.click_adds_pins && !modifiers.shift => {
                for pin in &mut warp.pins {
                    pin.picked = false;
                }
                let mut pin = crate::warp::Pin::at(local);
                pin.picked = true;
                warp.pins.push(pin);
                PinHow::Move
            }
            None => PinHow::Pick {
                add: modifiers.shift && settings.select_pins,
            },
        };
        self.gesture = Some(Gesture::Pin {
            drag: PinDrag {
                start: local,
                now: local,
                before: warp.pins.clone(),
                how,
            },
        });
    }

    /// Puppet warp's pins dragged to `pos`.
    pub(super) fn pin_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let step = if self.modifiers.shift {
            15_f32.to_radians()
        } else {
            0.0
        };
        let (Some(Gesture::Pin { drag }), Some(warp)) = (self.gesture.as_mut(), self.warp.as_mut())
        else {
            return;
        };
        let now = [p.x - warp.offset[0] as f32, p.y - warp.offset[1] as f32];
        drag.now = now;
        warp.pins = drag.applied(now, step);
        self.warp_preview(cx);
    }

    /// The Object tool pressed on a camera folder: inside the camera
    /// frame moves it, a corner or an edge zooms, the handle on its stalk
    /// (or outside the frame) turns it.
    pub(super) fn camera_press(&mut self, press: &Press, cx: &mut AppletContext<'_>) {
        let Press {
            pos,
            p,
            viewport,
            size,
            ..
        } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some(folder) = self.camera_folder(&cx.world.arena) else {
            return;
        };
        let base = self.camera_now(&cx.world.arena, folder);
        let quad = base.frame(size).map(|corner| {
            doc.view
                .to_screen(viewport, size, pos2(corner[0], corner[1]))
        });
        let (_, stalk) = crate::transform_ui::stalk_of(&quad, crate::canvas::marks::STALK);
        let grip = crate::transform_ui::frame_grip(&quad, stalk, pos, true);
        self.gesture = Some(Gesture::Camera {
            folder,
            start: [p.x, p.y],
            base,
            grip,
        });
    }

    /// The camera frame dragged to `pos`: pending until it is let go. Alt
    /// zooms about the middle instead of the other side; Shift keeps a
    /// move along one axis and turns in steps of 15 degrees.
    pub(super) fn camera_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        use crate::transform_ui::{FrameGrip, constrained, grip_points, scale_ratio};
        let Some(Gesture::Camera {
            folder,
            start,
            base,
            grip,
        }) = self.gesture
        else {
            return;
        };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let p = doc.view.to_picture(viewport, size, pos);
        let frame = doc.frame;
        let (alt, shift) = (self.modifiers.alt, self.modifiers.shift);
        let mut camera = base;
        let center = base.center;
        match grip {
            FrameGrip::Inside | FrameGrip::Centre => {
                let by = constrained([p.x - start[0], p.y - start[1]], shift);
                camera.center = [center[0] + by[0], center[1] + by[1]];
            }
            FrameGrip::Turn => {
                let step = if shift { 15_f32.to_radians() } else { 0.0 };
                camera.angle =
                    base.angle + crate::transform::drag_angle(center, start, [p.x, p.y], step);
            }
            FrameGrip::Corner(_) | FrameGrip::Edge(_) => {
                // A bigger frame sees more: the camera zooms out.
                let Some((held, anchor)) = grip_points(&base.frame(size), grip, alt) else {
                    return;
                };
                let k = scale_ratio(anchor, held, [p.x, p.y]);
                camera.scale = (base.scale / k).clamp(0.01, 20.0);
                let k = base.scale / camera.scale;
                camera.center = [
                    anchor[0] + (center[0] - anchor[0]) * k,
                    anchor[1] + (center[1] - anchor[1]) * k,
                ];
            }
        }
        self.camera_pending = Some((folder, frame, camera));
    }

    /// The camera frame let go.
    pub(super) fn camera_release(&mut self, cx: &mut AppletContext<'_>) {
        // Keyed at this frame, as one step.
        if self.camera_pending.is_some() {
            self.run("anim.camera.key", cx);
        }
    }

    /// A handle of the transform box dragged to `pos`.
    pub(super) fn transform_box_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        self.transform_drag([p.x, p.y], self.modifiers);
        self.transform_preview(cx);
    }
}

/// Where the layer's point `at` shows when it stands as `pose`: scaled and
/// turned about its centre, then moved (the inverse of `Pose::map`).
fn pose_forward(pose: &crate::anim::motion::Pose, at: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = pose.rotation.to_radians().sin_cos();
    let s = pose.scale.max(0.001);
    let [cx, cy] = pose.center;
    let (x, y) = (at[0] - cx, at[1] - cy);
    [
        cx + pose.position[0] + s * (cos * x - sin * y),
        cy + pose.position[1] + s * (sin * x + cos * y),
    ]
}

#[cfg(test)]
mod tests {
    use super::pose_forward;

    #[test]
    fn a_pose_puts_a_point_where_its_map_sees_it() {
        let pose = crate::anim::motion::Pose {
            position: [12.0, -7.0],
            scale: 1.5,
            rotation: 30.0,
            center: [40.0, 30.0],
            opacity: None,
        };
        let m = pose.map();
        for at in [[0.0, 0.0], [40.0, 30.0], [77.0, 12.0]] {
            let [x, y] = pose_forward(&pose, at);
            let back = [m[0] * x + m[1] * y + m[4], m[2] * x + m[3] * y + m[5]];
            assert!(
                (back[0] - at[0]).abs() < 1e-3 && (back[1] - at[1]).abs() < 1e-3,
                "{at:?} came back as {back:?}"
            );
        }
    }
}
