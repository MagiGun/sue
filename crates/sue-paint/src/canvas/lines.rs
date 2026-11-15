//! Vector lines on the canvas beyond the eraser of `tools/vector.rs`:
//!
//! - the Object tool's loop drawn round lines to pick several, and the
//!   box round what is picked, whose eight handles scale it (Mode:
//!   Scale) or move its corners one by one (Mode: Free transform), with
//!   Adjust line thickness when scaling;
//! - Tool Property's changes to picked lines (colors, brush size and
//!   shape), shown while a control is dragged and kept as one step when
//!   it is let go;
//! - figures laid on a vector layer as lines with the sub tool's brush;
//! - the Vector eraser over every picked vector layer at once;
//! - Layer, Update Lines to New Brush Engine.

use egui::{Painter, Pos2, Rect, Shape, Stroke, Vec2};
use sue_core::EntityId;
use sue_model::Geometry;
use sue_wm::ui::tokens::Space;
use sue_wm::{AppletContext, ArenaEdit, Call, Theme};

use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};
use crate::vector::{self, Erase, Line, Restyle};

/// A gesture on vector lines that this file runs.
#[derive(Debug)]
pub(crate) enum LinesGesture {
    /// A loop drawn round lines to pick them: the layer, the loop (layer
    /// pixels), and whether it adds to what is picked (Shift).
    Loop {
        layer: EntityId,
        path: Vec<[f32; 2]>,
        add: bool,
        offset: [f32; 2],
    },
    /// A handle of the picked lines' box dragged.
    Box(Box<BoxDrag>),
    /// The Vector eraser over several picked vector layers.
    Erase {
        layers: Vec<ErasedLayer>,
        /// Where the eraser last was, in picture pixels.
        last: [f32; 2],
        radius: f32,
        mode: Erase,
        label: String,
    },
}

/// A handle of the picked lines' box being dragged.
#[derive(Debug)]
pub(crate) struct BoxDrag {
    layer: EntityId,
    before: Vec<u8>,
    geometry: Geometry,
    base: Vec<Line>,
    picked: Vec<usize>,
    /// The box as the drag began (layer pixels).
    from: [f32; 4],
    /// Which handle: 0 to 3 the corners from the top left round, 4 to 7
    /// the middles of the top, right, bottom and left edges.
    handle: usize,
    start: [f32; 2],
    /// The box's corners now.
    to: [[f32; 2]; 4],
    offset: [f32; 2],
    free: bool,
    thicken: bool,
}

/// One layer under the Vector eraser over several layers.
#[derive(Debug)]
pub(crate) struct ErasedLayer {
    layer: EntityId,
    before: Vec<u8>,
    geometry: Geometry,
    lines: Vec<Line>,
    offset: [f32; 2],
    changed: bool,
}

/// What Tool Property shows while a control is dragged: the lines as they
/// were, so the change can be shown on them and kept as one step.
#[derive(Debug)]
pub(crate) struct LinePreview {
    layer: EntityId,
    before: Vec<u8>,
    geometry: Geometry,
    base: Vec<Line>,
}

/// A change Tool Property asks of the picked lines: shown now, and kept
/// as one step when `keep`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineChange {
    pub(crate) restyle: Restyle,
    pub(crate) keep: bool,
}

/// How near a handle a press must land, in screen points.
const HANDLE_REACH: f32 = 9.0;

/// Moves the corners `to` of a box for its `handle` dragged by `by`:
/// in Scale mode the box stays a box, the far corner or edge where it
/// is; in Free transform the corner (or the edge's two corners) moves
/// alone.
fn drag_corners(
    to: &mut [[f32; 2]; 4],
    from: &[[f32; 2]; 4],
    handle: usize,
    by: [f32; 2],
    free: bool,
) {
    let add = |a: [f32; 2], d: [f32; 2]| [a[0] + d[0], a[1] + d[1]];
    let x = [by[0], 0.0];
    let y = [0.0, by[1]];
    *to = *from;
    if free {
        let moved: &[usize] = match handle {
            0..=3 => &[handle][..],
            4 => &[0, 1],
            5 => &[1, 2],
            6 => &[2, 3],
            _ => &[3, 0],
        };
        let by = match handle {
            4 | 6 => y,
            5 | 7 => x,
            _ => by,
        };
        for corner in moved {
            to[*corner] = add(from[*corner], by);
        }
        return;
    }
    // Scale: the box's edges as the handle moves them.
    let [mut left, mut top] = from[0];
    let [mut right, mut bottom] = from[2];
    match handle {
        0 => (left, top) = (left + by[0], top + by[1]),
        1 => (right, top) = (right + by[0], top + by[1]),
        2 => (right, bottom) = (right + by[0], bottom + by[1]),
        3 => (left, bottom) = (left + by[0], bottom + by[1]),
        4 => top += by[1],
        5 => right += by[0],
        6 => bottom += by[1],
        _ => left += by[0],
    }
    *to = vector::corners([left, top, right, bottom]);
}

/// The eight handles of a box with corners `quad`: its corners, then the
/// middles of its edges from the top round.
fn handles(quad: &[[f32; 2]; 4]) -> [[f32; 2]; 8] {
    let middle = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    [
        quad[0],
        quad[1],
        quad[2],
        quad[3],
        middle(quad[0], quad[1]),
        middle(quad[1], quad[2]),
        middle(quad[2], quad[3]),
        middle(quad[3], quad[0]),
    ]
}

impl SuePaint {
    /// The picked lines of the vector layer `layer`, when some are.
    pub(crate) fn picked_on(&self, layer: EntityId) -> Option<Vec<usize>> {
        self.picked_lines
            .as_ref()
            .filter(|(on, picked)| *on == layer && !picked.is_empty())
            .map(|(_, picked)| picked.clone())
    }

    /// The Object tool pressed at `local` (layer pixels) on the vector
    /// `layer` with lines picked: a handle of their box starts scaling
    /// or bending it, a press inside the box (Shift not held) moves them.
    /// `true` when the press was the box's.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn line_box_press(
        &mut self,
        layer: EntityId,
        local: [f32; 2],
        zoom: f32,
        lines: &[Line],
        geometry: &Geometry,
        offset: [i64; 2],
        shift: bool,
        cx: &AppletContext<'_>,
    ) -> bool {
        let Some(picked) = self.picked_on(layer) else {
            return false;
        };
        let Some(from) = vector::bounds(lines, &picked) else {
            return false;
        };
        let Some(before) = doc::raster(&cx.world.arena, layer)
            .and_then(|raster| raster.bytes())
            .map(<[u8]>::to_vec)
        else {
            return false;
        };
        let reach = HANDLE_REACH / zoom.max(0.01);
        let quad = vector::corners(from);
        let near = handles(&quad)
            .iter()
            .enumerate()
            .map(|(index, at)| (index, (at[0] - local[0]).hypot(at[1] - local[1])))
            .filter(|(_, away)| *away <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index);
        if let Some(handle) = near {
            self.gesture = Some(Gesture::Lines(Box::new(LinesGesture::Box(Box::new(
                BoxDrag {
                    layer,
                    before,
                    geometry: geometry.clone(),
                    base: lines.to_vec(),
                    picked,
                    from,
                    handle,
                    start: local,
                    to: quad,
                    offset: offset.map(|value| value as f32),
                    free: self.line_free,
                    thicken: self.line_thicken,
                },
            )))));
            return true;
        }
        let inside = local[0] >= from[0] - reach
            && local[0] <= from[2] + reach
            && local[1] >= from[1] - reach
            && local[1] <= from[3] + reach;
        if inside && !shift {
            self.gesture = Some(Gesture::MoveLines {
                layer,
                before,
                geometry: geometry.clone(),
                base: lines.to_vec(),
                start: local,
                by: [0.0; 2],
                offset,
            });
            return true;
        }
        false
    }

    /// The Object tool pressed on no line of the vector `layer`: a drag
    /// from here draws a loop round the lines to pick (with Shift, to add
    /// to what is picked).
    pub(crate) fn line_loop_press(
        &mut self,
        layer: EntityId,
        local: [f32; 2],
        offset: [i64; 2],
        add: bool,
    ) {
        self.gesture = Some(Gesture::Lines(Box::new(LinesGesture::Loop {
            layer,
            path: vec![local],
            add,
            offset: offset.map(|value| value as f32),
        })));
    }

    /// A gesture of this file carried on to `pos` (on screen).
    pub(crate) fn lines_drag(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let shift = self.modifiers.shift;
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let Some(Gesture::Lines(gesture)) = self.gesture.as_mut() else {
            return;
        };
        let mut painted: Vec<(EntityId, Vec<Line>)> = Vec::new();
        match gesture.as_mut() {
            LinesGesture::Loop { path, offset, .. } => {
                let at = [p.x - offset[0], p.y - offset[1]];
                let step = 1.5 / view.zoom.max(0.01);
                if path
                    .last()
                    .is_none_or(|last| (last[0] - at[0]).hypot(last[1] - at[1]) > step)
                {
                    path.push(at);
                }
            }
            LinesGesture::Box(drag) => {
                let at = [p.x - drag.offset[0], p.y - drag.offset[1]];
                let by = [at[0] - drag.start[0], at[1] - drag.start[1]];
                let from = vector::corners(drag.from);
                // Shift keeps a corner's box in proportion while scaling.
                let by = if shift && !drag.free && drag.handle < 4 {
                    let (w, h) = (drag.from[2] - drag.from[0], drag.from[3] - drag.from[1]);
                    let sign = |corner: usize| match corner {
                        0 => [-1.0, -1.0],
                        1 => [1.0, -1.0],
                        2 => [1.0, 1.0],
                        _ => [-1.0, 1.0],
                    };
                    let s = sign(drag.handle);
                    let grow = ((w + by[0] * s[0]) / w.max(f32::EPSILON))
                        .max((h + by[1] * s[1]) / h.max(f32::EPSILON));
                    [(grow - 1.0) * w * s[0], (grow - 1.0) * h * s[1]]
                } else {
                    by
                };
                drag_corners(&mut drag.to, &from, drag.handle, by, drag.free);
                painted.push((
                    drag.layer,
                    vector::transformed(
                        &drag.base,
                        &drag.picked,
                        drag.from,
                        &drag.to,
                        drag.thicken,
                    ),
                ));
            }
            LinesGesture::Erase {
                layers,
                last,
                radius,
                mode,
                ..
            } => {
                let to = [p.x, p.y];
                let step = (*radius * 0.5).max(0.5);
                let length = (to[0] - last[0]).hypot(to[1] - last[1]);
                let count = (length / step).ceil().max(1.0) as usize;
                for layer in layers.iter_mut() {
                    let mut erased = false;
                    for n in 1..=count {
                        let t = n as f32 / count as f32;
                        let at = [
                            last[0] + (to[0] - last[0]) * t - layer.offset[0],
                            last[1] + (to[1] - last[1]) * t - layer.offset[1],
                        ];
                        if let Some(left) = vector::erase(&layer.lines, at, *radius, *mode) {
                            layer.lines = left;
                            erased = true;
                        }
                    }
                    if erased {
                        layer.changed = true;
                        painted.push((layer.layer, layer.lines.clone()));
                    }
                }
                *last = to;
            }
        }
        for (layer, lines) in &painted {
            Self::paint_lines(cx, *layer, lines);
        }
        if !painted.is_empty()
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// A gesture of this file ended: the loop picks, the box and the
    /// eraser keep what they did as one step.
    pub(crate) fn lines_release(&mut self, gesture: LinesGesture, cx: &mut AppletContext<'_>) {
        match gesture {
            LinesGesture::Loop {
                layer, path, add, ..
            } => {
                let lines = vector::lines(&vector::geometry(&cx.world.arena, layer));
                let found = vector::in_loop(&lines, &path);
                let mut picked = match self.picked_lines.take() {
                    Some((on, picked)) if on == layer && add => picked,
                    _ => Vec::new(),
                };
                for index in found {
                    if !picked.contains(&index) {
                        picked.push(index);
                    }
                }
                self.picked_lines = Some((layer, picked));
            }
            LinesGesture::Box(drag) => {
                let BoxDrag {
                    layer,
                    before,
                    geometry,
                    base,
                    picked,
                    from,
                    to,
                    thicken,
                    ..
                } = *drag;
                if to == vector::corners(from) {
                    return;
                }
                let lines = vector::transformed(&base, &picked, from, &to, thicken);
                Self::keep_lines(
                    cx,
                    layer,
                    &before,
                    geometry,
                    &lines,
                    &tr!("transform-lines"),
                );
            }
            LinesGesture::Erase { layers, label, .. } => {
                let mut step = crate::doc::Edit::new(&label);
                let mut any = false;
                for layer in layers.into_iter().filter(|layer| layer.changed) {
                    if let Some(edit) = Self::kept_lines(
                        cx,
                        layer.layer,
                        &layer.before,
                        layer.geometry,
                        &layer.lines,
                        &label,
                    ) {
                        step = step.then(edit);
                        any = true;
                    }
                }
                if any {
                    cx.record(ArenaEdit(step));
                }
            }
        }
    }

    /// Paints the vector `layer` again from `lines`, as it shows while a
    /// gesture goes on.
    fn paint_lines(cx: &mut AppletContext<'_>, layer: EntityId, lines: &[Line]) {
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer)
            && let Some(painted) = vector::render(lines, raster.width(), raster.height())
            && let (Some(into), Some(from)) = (raster.bytes_mut(), painted.bytes())
        {
            into.copy_from_slice(from);
        }
    }

    /// Paints the vector `layer` from `lines` and keeps them, with its
    /// pixels from `before`, as the edit named `label` (in place, not
    /// recorded); `None` when the layer has no pixels.
    fn kept_lines(
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        before: &[u8],
        geometry: Geometry,
        lines: &[Line],
        label: &str,
    ) -> Option<crate::doc::Edit> {
        Self::paint_lines(cx, layer, lines);
        let raster = doc::raster(&cx.world.arena, layer)?;
        let pixels = doc::diff_patch(before, raster.bytes()?, raster.width(), raster.height())
            .map(|patch| sue_core::txn::RasterEdit::new(layer, label, vec![patch]));
        Some(vector::keep(
            &mut cx.world.arena,
            layer,
            geometry,
            lines,
            pixels,
            label,
        ))
    }

    /// [`Self::kept_lines`], recorded as one undo step.
    fn keep_lines(
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        before: &[u8],
        geometry: Geometry,
        lines: &[Line],
        label: &str,
    ) {
        if let Some(edit) = Self::kept_lines(cx, layer, before, geometry, lines, label) {
            cx.record(ArenaEdit(edit));
        }
    }

    /// Tool Property's change to the picked lines, and keeping a shown
    /// change when nothing holds it any more (`held`: a pointer button
    /// is down). Called once a frame by Tool Property.
    pub(crate) fn line_changes(&mut self, held: bool, cx: &mut AppletContext<'_>) {
        let change = self.pending_lines.take();
        let Some(change) = change else {
            if !held && self.line_preview.is_some() {
                self.finish_line_preview(cx, None);
            }
            return;
        };
        let Some((layer, picked)) = self.picked_lines.clone() else {
            self.line_preview = None;
            return;
        };
        if change.restyle.is_empty() || picked.is_empty() {
            return;
        }
        if self
            .line_preview
            .as_ref()
            .is_some_and(|preview| preview.layer != layer)
        {
            self.finish_line_preview(cx, None);
        }
        if self.line_preview.is_none() {
            let Some(before) = doc::raster(&cx.world.arena, layer)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec)
            else {
                return;
            };
            let geometry = vector::geometry(&cx.world.arena, layer);
            self.line_preview = Some(LinePreview {
                layer,
                before,
                base: vector::lines(&geometry),
                geometry,
            });
        }
        let Some(preview) = self.line_preview.as_ref() else {
            return;
        };
        let lines = vector::restyled(&preview.base, &picked, &change.restyle);
        if change.keep {
            self.finish_line_preview(cx, Some(lines));
        } else {
            Self::paint_lines(cx, layer, &lines);
            self.line_lines_shown = Some(lines);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Keeps the change Tool Property showed (`lines`, or the ones shown
    /// last) as one step.
    fn finish_line_preview(&mut self, cx: &mut AppletContext<'_>, lines: Option<Vec<Line>>) {
        let Some(preview) = self.line_preview.take() else {
            return;
        };
        let shown = self.line_lines_shown.take();
        let Some(lines) = lines.or(shown) else {
            return;
        };
        if lines == preview.base {
            return;
        }
        Self::keep_lines(
            cx,
            preview.layer,
            &preview.before,
            preview.geometry,
            &lines,
            &tr!("line-style"),
        );
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// `Ok` for the current layer when the Figure tool's `sub` draws on
    /// it: a raster layer as always, or a vector layer for a figure that
    /// is a line (straight line, rectangle, ellipse, polyline, curve),
    /// which it lays as a vector line.
    pub(crate) fn figure_target(
        &self,
        arena: &sue_core::Arena,
        sub: &SubTool,
    ) -> Result<doc::LayerInfo, String> {
        match self.drawable(arena) {
            Ok(info) if vector::is_vector(arena, info.id) && Self::figure_is_line(sub) => Ok(info),
            _ => self.paintable(arena),
        }
    }

    /// `true` when the Figure sub tool `sub` draws a line, not a filled
    /// shape or a layer of its own.
    fn figure_is_line(sub: &SubTool) -> bool {
        sub.tool == Tool::Figure
            && matches!(
                sub.mode,
                0..=2 | crate::tools::POLYLINE | crate::tools::CURVE
            )
    }

    /// A figure on a vector layer: `points` (picture pixels) laid as one
    /// vector line with the sub tool's brush shape (its tapers off, as the
    /// Figure tool's lines on a raster layer), in the drawing color, as
    /// one step; with `corners` each point a corner Correct Line keeps
    /// sharp. `false` when the current layer is not a vector layer (the
    /// figure is painted as before).
    pub(crate) fn figure_on_vector(
        &mut self,
        points: &[[f32; 2]],
        corners: bool,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let arena = &cx.world.arena;
        let Some(info) = self
            .layer_info(arena)
            .filter(|info| vector::is_vector(arena, info.id))
        else {
            return false;
        };
        if let Err(error) = self.drawable(arena) {
            cx.fail(error);
            return true;
        }
        if points.len() < 2 {
            return true;
        }
        let Some(before) = doc::raster(arena, info.id)
            .and_then(|raster| raster.bytes())
            .map(<[u8]>::to_vec)
        else {
            return true;
        };
        let offset = info.style.offset.map(|value| value as f32);
        let local: Vec<[f32; 2]> = points
            .iter()
            .map(|p| [p[0] - offset[0], p[1] - offset[1]])
            .collect();
        let tip = crate::brush::Tip {
            taper_start: 0.0,
            taper_end: 0.0,
            mix: 0.0,
            eraser: false,
            ..sub.tip
        };
        let colors = [
            crate::color::unit(self.drawing_color()),
            crate::color::unit(if self.sub_active { self.main } else { self.sub }),
        ];
        let geometry = vector::geometry(arena, info.id);
        let mut lines = vector::lines(&geometry);
        let mut line = vector::figure_line(&local, tip, colors, corners);
        if self.vector_magnet {
            let reach = (tip.size * 0.5).max(MAGNET_REACH);
            vector::snap_ends(&mut line, &lines, reach);
        }
        lines.push(line);
        Self::keep_lines(cx, info.id, &before, geometry, &lines, &sub.name);
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
        true
    }

    /// The Vector eraser pressed at `p` (picture pixels) while other
    /// layers are picked beside the current one: it erases
    /// the lines of every picked vector layer it passes over, as one
    /// step. `false` when fewer than two picked layers are vector layers
    /// (the eraser works on the current layer as before).
    pub(crate) fn vector_erase_layers_press(
        &mut self,
        p: Pos2,
        tip_size: f32,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let arena = &cx.world.arena;
        let Some(doc) = self.docs.get(self.active) else {
            return false;
        };
        let Some(current) = doc.layer else {
            return false;
        };
        // The current layer is picked with the others `doc.picked` names.
        if doc.picked.is_empty() {
            return false;
        }
        let layers: Vec<ErasedLayer> = doc::layers(arena, doc.picture)
            .into_iter()
            .filter(|info| {
                (info.id == current || doc.picked.contains(&info.id))
                    && vector::is_vector(arena, info.id)
                    && !info.locked
                    && info.style.visible
            })
            .filter_map(|info| {
                let before = doc::raster(arena, info.id)?.bytes()?.to_vec();
                let geometry = vector::geometry(arena, info.id);
                Some(ErasedLayer {
                    layer: info.id,
                    before,
                    lines: vector::lines(&geometry),
                    geometry,
                    offset: info.style.offset.map(|value| value as f32),
                    changed: false,
                })
            })
            .collect();
        if layers.len() < 2 {
            return false;
        }
        self.gesture = Some(Gesture::Lines(Box::new(LinesGesture::Erase {
            layers,
            last: [p.x, p.y],
            radius: (tip_size / 2.0).max(0.5),
            mode: Erase::of(sub.kind),
            label: crate::localize(&sub.name),
        })));
        true
    }

    /// The picked lines over the canvas: each line traced, their box with
    /// its eight handles (as it is being dragged), and the loop being
    /// drawn, in the theme's colors with a dark edge so they read on any
    /// picture.
    pub(crate) fn lines_overlay(
        &self,
        painter: &Painter,
        screen: &dyn Fn([f32; 2]) -> Pos2,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some(current) = doc.layer else {
            return;
        };
        let light = Stroke::new(theme.border_width, theme.border_active);
        let dark = Stroke::new(theme.border_width + Theme::STRONG_STROKE, theme.shadow);
        let offset_of = |layer: EntityId| {
            doc::layers(arena, doc.picture)
                .into_iter()
                .find(|info| info.id == layer)
                .map_or([0.0; 2], |info| info.style.offset.map(|value| value as f32))
        };
        if let Some(Gesture::Lines(gesture)) = &self.gesture
            && let LinesGesture::Loop { path, offset, .. } = gesture.as_ref()
            && path.len() > 1
        {
            let points: Vec<Pos2> = path
                .iter()
                .map(|at| screen([at[0] + offset[0], at[1] + offset[1]]))
                .collect();
            painter.add(Shape::line(points.clone(), dark));
            painter.add(Shape::line(points, light));
        }
        let Some(picked) = self.picked_on(current) else {
            return;
        };
        let offset = offset_of(current);
        let at = |p: [f32; 2]| screen([p[0] + offset[0], p[1] + offset[1]]);
        let base = vector::lines(&vector::geometry(arena, current));
        let (lines, quad) = match &self.gesture {
            Some(Gesture::MoveLines { base, by, .. }) => {
                let moved = vector::moved(base, &picked, *by);
                let quad = vector::bounds(&moved, &picked).map(vector::corners);
                (moved, quad)
            }
            Some(Gesture::Lines(gesture)) => match gesture.as_ref() {
                LinesGesture::Box(drag) => (
                    vector::transformed(
                        &drag.base,
                        &drag.picked,
                        drag.from,
                        &drag.to,
                        drag.thicken,
                    ),
                    Some(drag.to),
                ),
                _ => {
                    let quad = vector::bounds(&base, &picked).map(vector::corners);
                    (base, quad)
                }
            },
            _ => {
                let quad = vector::bounds(&base, &picked).map(vector::corners);
                (base, quad)
            }
        };
        // A hairline along each picked line's middle, so its ink still
        // shows round it.
        for line in picked.iter().filter_map(|index| lines.get(*index)) {
            let points: Vec<Pos2> = line.samples.iter().map(|sample| at(sample.at)).collect();
            painter.add(Shape::line(points, light));
        }
        let Some(quad) = quad else {
            return;
        };
        let corners: Vec<Pos2> = quad.iter().map(|p| at(*p)).collect();
        painter.add(Shape::closed_line(corners.clone(), dark));
        painter.add(Shape::closed_line(corners, light));
        let side = theme.space(Space::S4);
        for handle in handles(&quad) {
            let square = Rect::from_center_size(at(handle), Vec2::splat(side));
            painter.rect_filled(
                square,
                theme.radius(sue_wm::ui::tokens::Radius::Xs),
                theme.text,
            );
            painter.rect_stroke(
                square,
                theme.radius(sue_wm::ui::tokens::Radius::Xs),
                Stroke::new(theme.border_width, theme.shadow),
                egui::StrokeKind::Outside,
            );
        }
    }
}

/// Correct Line's control point `index` of `lines[line]` let go: when it
/// is an end of its line and lies within `reach` of the line's other end
/// or of another line's end, it goes onto it, so dragging an open end
/// onto the other closes the shape. `true` when it moved.
pub(crate) fn join_dragged_end(lines: &mut [Line], line: usize, index: usize, reach: f32) -> bool {
    let Some(count) = lines.get(line).map(|line| line.samples.len()) else {
        return false;
    };
    if index != 0 && index + 1 != count {
        return false;
    }
    let others: Vec<Line> = lines
        .iter()
        .enumerate()
        .filter(|(at, _)| *at != line)
        .map(|(_, other)| other.clone())
        .collect();
    vector::snap_end(&mut lines[line], index != 0, &others, reach, true)
}

/// The least distance, in layer pixels, the Vector magnet reaches.
const MAGNET_REACH: f32 = 6.0;

/// Layer, Update Lines to New Brush Engine: the legacy lines of the
/// current vector layer become lines of the brush engine, as one step;
/// a line the engine cannot yet draw as it looks (a flick, post
/// correction) stays as it is, and the notice says how many did.
pub(crate) fn update_lines(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(layer) = paint
        .layer()
        .filter(|layer| vector::is_vector(&cx.world.arena, *layer))
    else {
        return;
    };
    let label = tr!("update-lines-to-new-brush-engine");
    let waiting = vector::legacy_lines(&cx.world.arena, layer);
    match vector::update_lines_kept(&mut cx.world.arena, layer, &label) {
        Some((edit, did)) => {
            cx.record(ArenaEdit(edit));
            if let Some(doc) = paint.doc_mut() {
                doc.compositor
                    .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
            }
            if did.kept > 0 {
                cx.notify(tr!(
                    "updated-lines-kept-some",
                    updated = did.updated,
                    kept = did.kept
                ));
            } else {
                cx.notify(tr!("updated-lines", updated = did.updated));
            }
        }
        None if waiting.kept > 0 => {
            cx.notify(tr!("no-lines-updated-kept", kept = waiting.kept));
        }
        None => cx.notify(tr!("no-legacy-lines-to-update")),
    }
}

/// While the current layer is a vector layer.
pub(crate) fn on_vector_layer(
    paint: &SuePaint,
    facts: &sue_wm::Facts<'_>,
    _: Call<'_>,
) -> sue_wm::ItemState {
    if paint
        .layer()
        .is_some_and(|layer| vector::is_vector(facts.arena, layer))
    {
        sue_wm::ItemState::on()
    } else {
        sue_wm::ItemState::off()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_handles_keep_a_box_and_free_handles_bend_it() {
        let from = vector::corners([0.0, 0.0, 10.0, 10.0]);
        let mut to = from;
        // The bottom middle pulled down 5: a taller box.
        drag_corners(&mut to, &from, 6, [3.0, 5.0], false);
        assert_eq!(to, vector::corners([0.0, 0.0, 10.0, 15.0]));
        // The top left corner in Scale: the bottom right stays.
        drag_corners(&mut to, &from, 0, [-2.0, -4.0], false);
        assert_eq!(to, vector::corners([-2.0, -4.0, 10.0, 10.0]));
        // Free transform: the bottom right corner alone.
        drag_corners(&mut to, &from, 2, [4.0, 1.0], true);
        assert_eq!(to, [[0.0, 0.0], [10.0, 0.0], [14.0, 11.0], [0.0, 10.0]]);
        // Free transform: the right edge's two corners, sideways only.
        drag_corners(&mut to, &from, 5, [4.0, 9.0], true);
        assert_eq!(to, [[0.0, 0.0], [14.0, 0.0], [14.0, 10.0], [0.0, 10.0]]);
        assert_eq!(handles(&from)[5], [10.0, 5.0]);
    }
}
