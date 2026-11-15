//! The Operation tool on the canvas. Object picks vector lines (Shift adds
//! or takes away) and drags what is picked, moves a raster layer, and
//! hands frame border and effect line layers, 3D layers and the camera
//! frame to their own handles, as its Selectable object says. Select layer
//! picks the layer painted where it is clicked, or every layer painted in
//! the box it drags out. Light table drags the light table's layers, and
//! Edit timeline steps through the frames by a click or a drag.

use egui::{Modifiers, Pos2, Rect};
use sue_core::EntityId;
use sue_wm::{AppletContext, ArenaEdit};

use super::Press;
use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{
    OPERATION_EDIT_TIMELINE, OPERATION_LIGHT_TABLE, PICKS_3D, PICKS_CAMERA, PICKS_COMIC,
    PICKS_RASTER, PICKS_VECTOR, SubTool, Tool,
};

/// How far a Select layer drag may wander, in screen points, and still be
/// a click.
const CLICK: f32 = 4.0;

/// Edit timeline's click: 0 steps to the next frame, 1 back (its Tool
/// Property).
pub(crate) const TIMELINE_BACK: u8 = 1;

/// The Light table sub tool's drag: 0 moves the light table's layers, 1
/// turns them (Shift the other one).
pub(crate) const LIGHT_TABLE_TURNS: u8 = 1;

impl SuePaint {
    /// The Operation tool pressed.
    pub(super) fn operation_press(
        &mut self,
        press: &Press,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Press {
            pos, p, modifiers, ..
        } = *press;
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let (zoom, frame) = (doc.view.zoom, doc.frame);
        match sub.mode {
            1 => {
                self.gesture = Some(Gesture::Shape {
                    tool: Tool::Operation,
                    sub: Box::new(sub.clone()),
                    op: press.op,
                    from: p,
                    to: p,
                });
            }
            OPERATION_LIGHT_TABLE => {
                let turn = (sub.kind == LIGHT_TABLE_TURNS) != modifiers.shift;
                let held = Modifiers {
                    shift: turn,
                    ..Modifiers::NONE
                };
                if !self.light_table_press(p, held, cx) {
                    cx.notify(tr!("light-table-nothing-to-move"));
                }
            }
            OPERATION_EDIT_TIMELINE => {
                if self.timeline_here(&cx.world.arena).is_none() {
                    cx.notify(tr!("edit-timeline-needs-a-timeline"));
                    return;
                }
                self.gesture = Some(Gesture::Scrub {
                    tool: Tool::Operation,
                    sub: Box::new(sub.clone()),
                    start: pos,
                    base: frame as f32,
                    moved: false,
                });
            }
            _ => self.object_press(press, sub, zoom, cx),
        }
    }

    /// The Object sub tool pressed: on what the layer in hand is, as far
    /// as its Selectable object lets it.
    fn object_press(
        &mut self,
        press: &Press,
        sub: &SubTool,
        zoom: f32,
        cx: &mut AppletContext<'_>,
    ) {
        let Press { p, modifiers, .. } = *press;
        let picks = |bit: u8| sub.objects & bit != 0;
        if picks(PICKS_COMIC) && self.on_comic_layer(&cx.world.arena) {
            // Frame borders and effect lines: their panels, corners and
            // ends move.
            let reach = 10.0 / zoom.max(0.01);
            self.gesture = self.comic_object_at(p, reach, modifiers.shift, &cx.world.arena);
            return;
        }
        if picks(PICKS_3D)
            && self
                .layer()
                .is_some_and(|layer| crate::three::is_3d(&cx.world.arena, layer))
        {
            self.three_press(press, cx);
            return;
        }
        if picks(PICKS_CAMERA) && self.camera_folder(&cx.world.arena).is_some() {
            self.camera_press(press, cx);
            return;
        }
        if self.keyed_box(&cx.world.arena).is_some() {
            // A layer whose keyframes are on: its box's handles key it.
            self.keyed_press(press, cx);
            return;
        }
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return;
        };
        let vector = crate::vector::is_vector(&cx.world.arena, info.id);
        if !vector || !picks(PICKS_VECTOR) {
            self.picked_lines = None;
            if !vector && picks(PICKS_RASTER) && info.kind == sue_model::picture::LayerKind::Raster
            {
                self.object_raster_press(press, sub, info.id, cx);
            } else {
                cx.notify(tr!("object-picks-vector-lines-pick-vector"));
            }
            return;
        }
        // Object on vector lines: click a line to pick it (Shift adds or
        // takes away), drag to move what is picked.
        let offset = info.style.offset;
        let local = [p.x - offset[0] as f32, p.y - offset[1] as f32];
        let geometry = crate::vector::geometry(&cx.world.arena, info.id);
        let lines = crate::vector::lines(&geometry);
        let reach = 8.0 / zoom.max(0.01);
        // The picked lines' box: its handles, and a drag inside it.
        if self.line_box_press(
            info.id,
            local,
            zoom,
            &lines,
            &geometry,
            offset,
            modifiers.shift,
            cx,
        ) {
            return;
        }
        let hit = crate::correct::line_at(&lines, local, reach).map(|(line, _)| line);
        let mut picked = match self.picked_lines.take() {
            Some((layer, picked)) if layer == info.id => picked,
            _ => Vec::new(),
        };
        match hit {
            None => {
                if !modifiers.shift {
                    picked.clear();
                }
                // A drag from here draws a loop round lines to pick.
                self.line_loop_press(info.id, local, offset, modifiers.shift);
            }
            Some(line) if modifiers.shift => {
                if let Some(at) = picked.iter().position(|index| *index == line) {
                    picked.remove(at);
                } else {
                    picked.push(line);
                }
            }
            Some(line) => {
                if !picked.contains(&line) {
                    picked = vec![line];
                }
            }
        }
        let moving = hit.is_some() && !picked.is_empty();
        self.picked_lines = Some((info.id, picked));
        if moving
            && let Some(before) = doc::raster(&cx.world.arena, info.id)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec)
        {
            self.gesture = Some(Gesture::MoveLines {
                layer: info.id,
                before,
                geometry,
                base: lines,
                start: local,
                by: [0.0; 2],
                offset,
            });
        }
    }

    /// Object on raster layer `layer`: a drag moves it, as Move layer
    /// does (the selected pixels when there is a selection). Where the
    /// click lands on no paint of it, Operation of transparent part says
    /// what happens: it moves all the same, the layer painted there is
    /// picked and moved, or nothing.
    fn object_raster_press(
        &mut self,
        press: &Press,
        sub: &SubTool,
        layer: EntityId,
        cx: &mut AppletContext<'_>,
    ) {
        let painted = self
            .layers_painted_in(
                &cx.world.arena,
                [[press.p.x.floor() as i64, press.p.y.floor() as i64]; 2],
                crate::fill::Exclude {
                    drafts: false,
                    text: false,
                    paper: false,
                    locked: false,
                    editing: false,
                },
            )
            .contains(&layer);
        let mut moving = sub.clone();
        moving.move_clicked = false;
        if !painted {
            match sub.transparent {
                1 => {
                    moving.move_clicked = true;
                    moving.move_select = true;
                }
                2 => return,
                _ => {}
            }
        }
        self.move_press(press, &moving, cx);
    }

    /// Select layer's click or box let go, from `from` to `to` (picture
    /// pixels): a click picks the layer painted there; a box picks every
    /// layer painted in it, the topmost the current one.
    pub(super) fn select_layers_in(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let zoom = self.doc().map_or(1.0, |doc| doc.view.zoom);
        if from.distance(to) * zoom <= CLICK {
            self.pick_layer_at(&cx.world.arena, from, sub.exclude);
            return;
        }
        let corner = |p: Pos2| [p.x.floor() as i64, p.y.floor() as i64];
        let hits = self.layers_painted_in(&cx.world.arena, [corner(from), corner(to)], sub.exclude);
        if let Some((first, rest)) = hits.split_first()
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(*first);
            doc.picked = rest.to_vec();
        }
    }

    /// Edit timeline's click: one frame on, or back, as it says.
    pub(super) fn timeline_click(&mut self, sub: &SubTool, cx: &mut AppletContext<'_>) {
        let Some(frame) = self.doc().map(|doc| i64::from(doc.frame)) else {
            return;
        };
        let step = if sub.kind == TIMELINE_BACK { -1 } else { 1 };
        self.go_to_frame(&cx.world.arena, frame + step);
    }

    /// The vector lines picked, dragged to `pos`.
    pub(super) fn move_lines_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(Gesture::MoveLines {
            layer,
            base,
            start,
            by,
            offset,
            ..
        }) = self.gesture.as_mut()
        else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        *by = [
            p.x - offset[0] as f32 - start[0],
            p.y - offset[1] as f32 - start[1],
        ];
        let picked = self
            .picked_lines
            .as_ref()
            .map(|(_, picked)| picked.clone())
            .unwrap_or_default();
        let lines = crate::vector::moved(base, &picked, *by);
        let layer = *layer;
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = crate::vector::render(&lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// The vector lines picked let go: the move as one undo step.
    pub(super) fn move_lines_release(&mut self, gesture: Gesture, cx: &mut AppletContext<'_>) {
        let Gesture::MoveLines {
            layer,
            before,
            geometry,
            base,
            by,
            ..
        } = gesture
        else {
            return;
        };
        if by == [0.0; 2] {
            return;
        }
        let picked = self
            .picked_lines
            .as_ref()
            .map(|(_, picked)| picked.clone())
            .unwrap_or_default();
        let lines = crate::vector::moved(&base, &picked, by);
        let pixels = doc::raster(&cx.world.arena, layer).and_then(|raster| {
            let patch = doc::diff_patch(&before, raster.bytes()?, raster.width(), raster.height())?;
            Some(sue_core::txn::RasterEdit::new(
                layer,
                tr!("move-lines"),
                vec![patch],
            ))
        });
        let edit = crate::vector::keep(
            &mut cx.world.arena,
            layer,
            geometry,
            &lines,
            pixels,
            &tr!("move-lines"),
        );
        cx.record(ArenaEdit(edit));
    }
}
