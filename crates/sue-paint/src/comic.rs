//! Sue Paint's side of manga (Clip Studio Paint's frame borders and
//! balloons): frame border layers made from a layout, panels drawn as
//! rectangles or divided by dragging a line, and balloon layers with
//! balloons and tails. Every edit is one undo step that keeps the shapes
//! and the drawing together.

use egui::Pos2;
use sue_core::txn::{ReplaceComponent, SetProperty, SpawnEntity};
use sue_core::{CHILD, Entity, EntityId, PixelFormat, RasterTile2D, Relation, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::AppletContext;

use crate::doc;
use crate::manga::{self, Panel, Shape};
use crate::studio::SuePaint;
use crate::tools::SubTool;

type Edit = sue_core::txn::Batch<sue_core::Arena>;

/// The Balloon tool's sub tools, by mode (0 is the ellipse balloon, 1 the
/// tail).
pub(crate) const ROUNDED_BALLOON: u8 = 2;
pub(crate) const CLOUD_BALLOON: u8 = 3;
pub(crate) const FLASH_BALLOON: u8 = 4;
pub(crate) const BALLOON_PEN: u8 = 5;
pub(crate) const THOUGHT_TAIL: u8 = 6;
pub(crate) const ELLIPSE_BALLOON: u8 = 0;
pub(crate) const CURVE_BALLOON: u8 = 7;
pub(crate) const POLYLINE_BALLOON: u8 = 8;
/// The Frame Border tool's polyline frame (0 is the rectangle, 1 the
/// division).
pub(crate) const POLYLINE_FRAME: u8 = 2;

/// Whether `tool` in `mode` lays corners one click at a time (closed by
/// clicking the first or last corner again, or Enter).
pub(crate) fn clicks_corners(tool: crate::tools::Tool, mode: u8) -> bool {
    use crate::tools::Tool;
    match tool {
        Tool::Balloon => matches!(mode, CURVE_BALLOON | POLYLINE_BALLOON),
        Tool::Frame => mode == POLYLINE_FRAME,
        Tool::Select => mode == 3,
        _ => false,
    }
}

/// An Object tool drag on a frame border or effect line layer.
#[derive(Clone, Debug)]
pub(crate) enum ComicEdit {
    /// A panel moved, or one of its corners.
    Panel {
        layer: EntityId,
        base: Vec<Panel>,
        now: Vec<Panel>,
        panel: usize,
        corner: Option<usize>,
        start: [f32; 2],
    },
    /// Effect lines' ends moved: `handle` 0 or 1 one end, 2 both.
    Effect {
        layer: EntityId,
        base: manga::EffectLines,
        now: manga::EffectLines,
        handle: u8,
        start: [f32; 2],
    },
}

impl ComicEdit {
    /// The pointer is at `p` (picture pixels).
    pub(crate) fn to(&mut self, p: Pos2) {
        match self {
            Self::Panel {
                base,
                now,
                panel,
                corner,
                start,
                ..
            } => {
                let by = [p.x - start[0], p.y - start[1]];
                let mut moved = base.clone();
                if let Some(points) = moved.get_mut(*panel) {
                    for (index, point) in points.iter_mut().enumerate() {
                        if corner.is_none_or(|corner| corner == index) {
                            *point = [point[0] + by[0], point[1] + by[1]];
                        }
                    }
                }
                *now = moved;
            }
            Self::Effect {
                base,
                now,
                handle,
                start,
                ..
            } => {
                let by = [p.x - start[0], p.y - start[1]];
                let shift = |q: [f32; 2]| [q[0] + by[0], q[1] + by[1]];
                *now = *base;
                if *handle != 1 {
                    now.a = shift(base.a);
                }
                if *handle != 0 {
                    now.b = shift(base.b);
                }
            }
        }
    }

    /// What to outline on the canvas while dragging (closed shapes).
    pub(crate) fn outlines(&self) -> Vec<Vec<[f32; 2]>> {
        match self {
            Self::Panel { now, panel, .. } => now.get(*panel).cloned().into_iter().collect(),
            Self::Effect { now, .. } => vec![effect_outline(now)],
        }
    }
}

/// Effect lines' handles as drawn: the clear ellipse of saturated lines,
/// or the arrow of speed lines.
pub(crate) fn effect_outline(lines: &manga::EffectLines) -> Vec<[f32; 2]> {
    let (a, b) = (lines.a, lines.b);
    if lines.speed {
        return vec![a, b];
    }
    let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
    let (rx, ry) = ((b[0] - a[0]).abs() / 2.0, (b[1] - a[1]).abs() / 2.0);
    (0..48)
        .map(|index| {
            let angle = index as f32 / 48.0 * std::f32::consts::TAU;
            [cx + rx * angle.cos(), cy + ry * angle.sin()]
        })
        .collect()
}
/// The Figure tool's effect lines, by mode.
pub(crate) const SATURATED_LINES: u8 = 5;
pub(crate) const SPEED_LINES: u8 = 6;

/// A frame border layer's settings when it is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameSpec {
    /// Which of [`manga::LAYOUTS`].
    pub layout: usize,
    /// Border width in pixels.
    pub border: f32,
    /// Space from the page's edges in pixels.
    pub margin: f32,
    /// Gap between panels across, in pixels.
    pub gap: f32,
    /// Gap between rows, in pixels.
    pub gap_y: f32,
}

impl Default for FrameSpec {
    fn default() -> Self {
        Self {
            layout: 5,
            border: 6.0,
            margin: 60.0,
            gap: 20.0,
            gap_y: 40.0,
        }
    }
}

impl SuePaint {
    /// The current picture, its size and its paper's color.
    fn page(&self, arena: &sue_core::Arena) -> Option<(EntityId, [u32; 2], [u8; 3])> {
        let picture = self.picture()?;
        let size = picture::size(arena.get_by_id(picture).ok()?)?;
        let paper = doc::layers(arena, picture)
            .into_iter()
            .find(|info| info.kind == LayerKind::Paper)
            .and_then(|info| arena.get_by_id(info.id).ok())
            .map_or([255, 255, 255], |entity| {
                crate::color::bytes(picture::paper_color(entity).rgb)
            });
        Some((picture, size, paper))
    }

    /// The topmost layer of class `class` in the current picture: the
    /// current layer if it is one.
    fn layer_of_class(&self, arena: &sue_core::Arena, class: &str) -> Option<EntityId> {
        let picture = self.picture()?;
        let is = |id: EntityId| {
            arena
                .get_by_id(id)
                .is_ok_and(|entity| entity.class() == class)
        };
        self.doc()
            .and_then(|doc| doc.layer)
            .filter(|id| is(*id))
            .or_else(|| {
                doc::layers(arena, picture)
                    .into_iter()
                    .rev()
                    .map(|info| info.id)
                    .find(|id| is(*id))
            })
    }

    /// Spawns a layer of `class` drawn as `pixels`, on top, and makes it
    /// the current layer.
    pub(crate) fn spawn_drawn_layer(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        class: &str,
        pixels: Vec<u8>,
        properties: Vec<(&str, Value)>,
    ) -> Option<EntityId> {
        let (picture, [width, height], _) = self.page(&cx.world.arena)?;
        let raster = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()?;
        let mut layer = Entity::new(name, raster).with_class(class);
        for (key, value) in properties {
            layer = layer.with_property(key, value);
        }
        let id = layer.id();
        let order = cx.world.arena.relations().next_order(picture, CHILD);
        let edit = Edit::new(tr!("new", name = name)).then(
            SpawnEntity::new(layer).with_relation(Relation::new(picture, CHILD, id).ordered(order)),
        );
        if !Self::apply(cx, edit) {
            return None;
        }
        self.sync_docs(&cx.world.arena);
        if let Some(doc) = self.doc_mut() {
            doc.layer = Some(id);
        }
        Some(id)
    }

    /// Makes a frame border layer with `spec`'s layout.
    pub(crate) fn add_frame_layer(&mut self, cx: &mut AppletContext<'_>, spec: FrameSpec) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            cx.fail(tr!("open-a-page-first"));
            return;
        };
        let rows = manga::LAYOUTS[spec.layout.min(manga::LAYOUTS.len() - 1)].1;
        let panels = manga::layout(
            rows,
            width as f32,
            height as f32,
            spec.margin,
            spec.gap,
            spec.gap_y,
        );
        let pixels = manga::render_frames(&panels, width, height, spec.border, paper);
        self.spawn_drawn_layer(
            cx,
            &tr!("frame"),
            manga::FRAME,
            pixels,
            vec![
                (manga::PANELS, Value::from(manga::encode_panels(&panels))),
                (manga::BORDER, Value::from(f64::from(spec.border))),
                (manga::GUTTER, Value::from(f64::from(spec.gap))),
            ],
        );
    }

    /// A frame layer's panels, border and gutter.
    fn frame_of(arena: &sue_core::Arena, layer: EntityId) -> Option<(Vec<Panel>, f32, f32)> {
        let entity = arena.get_by_id(layer).ok()?;
        let number = |key, default: f64| {
            entity
                .property(key)
                .and_then(Value::as_f64)
                .unwrap_or(default) as f32
        };
        let panels = entity
            .property(manga::PANELS)
            .and_then(Value::as_str)
            .map(manga::decode_panels)
            .unwrap_or_default();
        Some((
            panels,
            number(manga::BORDER, 6.0),
            number(manga::GUTTER, 20.0),
        ))
    }

    /// Gives a drawn layer new shapes: the property and the drawing, as
    /// one undo step.
    fn redraw_layer(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        label: &str,
        key: &str,
        text: String,
        pixels: Vec<u8>,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let Ok(raster) = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels) else {
            return;
        };
        let edit = Edit::new(label)
            .then(SetProperty::new(layer, key, Value::from(text)))
            .then(ReplaceComponent::new(layer, raster, label));
        Self::apply(cx, edit);
    }

    /// A Frame Border tool drag from `from` to `to` (picture pixels): a
    /// new rectangle panel, or a panel divided along the line.
    pub(crate) fn frame_drag(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            return;
        };
        let (a, mut b) = ([from.x, from.y], [to.x, to.y]);
        let existing = self.layer_of_class(&cx.world.arena, manga::FRAME);
        if sub.mode == 1 {
            // Frame border division: straight across when nearly so.
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            if dy.abs() < dx.abs() * 0.14 {
                b[1] = a[1];
            } else if dx.abs() < dy.abs() * 0.14 {
                b[0] = a[0];
            }
            let Some(layer) = existing else {
                cx.fail(tr!("make-frame-borders-first-layer-new"));
                return;
            };
            let Some((panels, border, gutter)) = Self::frame_of(&cx.world.arena, layer) else {
                return;
            };
            // Divide along the whole line through the drag.
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            let reach = (width + height) as f32;
            let length = ex.hypot(ey).max(1e-3);
            let middle = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            let (start, end) = (
                [
                    middle[0] - ex / length * reach,
                    middle[1] - ey / length * reach,
                ],
                [
                    middle[0] + ex / length * reach,
                    middle[1] + ey / length * reach,
                ],
            );
            let Some(divided) = manga::divide(&panels, start, end, gutter).map(|mut divided| {
                // Only the panel under the drag's middle is cut.
                divided.retain(|panel| panel.len() >= 3);
                divided
            }) else {
                cx.notify(tr!("drag-across-panel-divide"));
                return;
            };
            let pixels = manga::render_frames(&divided, width, height, border, paper);
            self.redraw_layer(
                cx,
                layer,
                &tr!("divide-frame"),
                manga::PANELS,
                manga::encode_panels(&divided),
                pixels,
            );
            return;
        }
        if (b[0] - a[0]).abs() < 4.0 || (b[1] - a[1]).abs() < 4.0 {
            return;
        }
        self.add_panel(cx, manga::rectangle(a, b), &tr!("rectangle-frame"));
    }

    /// A Balloon tool drag: a new balloon of the sub tool's shape, or a
    /// tail (or thought bubbles) from the balloon under `from` to `to`.
    pub(crate) fn balloon_drag(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let (a, b) = ([from.x, from.y], [to.x, to.y]);
        let shapes = self.balloon_shapes(&cx.world.arena);
        let under = shapes.iter().rev().find_map(|shape| {
            shape
                .balloon()
                .filter(|[x0, y0, x1, y1]| a[0] > *x0 && a[0] < *x1 && a[1] > *y0 && a[1] < *y1)
        });
        let new = match sub.mode {
            1 | THOUGHT_TAIL => {
                let Some(balloon) = under else {
                    cx.fail(tr!("start-tail-inside-balloon"));
                    return;
                };
                if sub.mode == THOUGHT_TAIL {
                    manga::bubbles(balloon, b)
                } else {
                    // A tail about a third as wide as the balloon it leaves.
                    let [x0, y0, x1, y1] = balloon;
                    vec![manga::tail(a, b, ((x1 - x0).min(y1 - y0) * 0.35).max(8.0))]
                }
            }
            _ => {
                if (b[0] - a[0]).abs() < 4.0 || (b[1] - a[1]).abs() < 4.0 {
                    return;
                }
                let rect = [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[0].max(b[0]),
                    a[1].max(b[1]),
                ];
                vec![match sub.mode {
                    ROUNDED_BALLOON => Shape::Rounded(rect),
                    CLOUD_BALLOON => Shape::Cloud(rect),
                    FLASH_BALLOON => Shape::Flash(rect),
                    _ => Shape::Ellipse(rect),
                }]
            }
        };
        if new.is_empty() {
            return;
        }
        let label = if matches!(sub.mode, 1 | THOUGHT_TAIL) {
            tr!("balloon-tail")
        } else {
            tr!("balloon")
        };
        self.add_balloon_shapes(cx, new, sub, &label);
    }

    /// The Balloon pen: a balloon in the outline drawn.
    pub(crate) fn balloon_drawn(
        &mut self,
        points: Vec<[f32; 2]>,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        // A few pixels apart is plenty for an outline.
        let mut outline: Vec<[f32; 2]> = Vec::new();
        for point in points {
            if outline
                .last()
                .is_none_or(|last| (last[0] - point[0]).hypot(last[1] - point[1]) >= 3.0)
            {
                outline.push(point);
            }
        }
        if outline.len() < 3 {
            return;
        }
        self.add_balloon_shapes(cx, vec![Shape::Drawn(outline)], sub, &tr!("balloon"));
    }

    /// The balloon layer's shapes (none without one).
    fn balloon_shapes(&self, arena: &sue_core::Arena) -> Vec<Shape> {
        self.layer_of_class(arena, manga::BALLOON)
            .and_then(|layer| arena.get_by_id(layer).ok())
            .and_then(|entity| {
                entity
                    .property(manga::SHAPES)
                    .and_then(Value::as_str)
                    .map(manga::decode_shapes)
            })
            .unwrap_or_default()
    }

    /// Adds `new` shapes to the balloon layer (a new one when there is
    /// none) as one undo step called `label`.
    fn add_balloon_shapes(
        &mut self,
        cx: &mut AppletContext<'_>,
        new: Vec<Shape>,
        sub: &SubTool,
        label: &str,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let line = sub.tip.size.max(1.0);
        let existing = self.layer_of_class(&cx.world.arena, manga::BALLOON);
        let mut shapes = self.balloon_shapes(&cx.world.arena);
        shapes.extend(new);
        let pixels = manga::render_balloons(&shapes, width, height, line);
        let text = manga::encode_shapes(&shapes);
        match existing {
            Some(layer) => {
                self.redraw_layer(cx, layer, label, manga::SHAPES, text, pixels);
            }
            None => {
                self.spawn_drawn_layer(
                    cx,
                    &tr!("balloon"),
                    manga::BALLOON,
                    pixels,
                    vec![
                        (manga::SHAPES, Value::from(text)),
                        (manga::LINE, Value::from(f64::from(line))),
                    ],
                );
            }
        }
    }

    /// The Figure tool's Saturated line (lines from all round stopping at
    /// the ellipse dragged) or Speed line (lines along the drag), as in
    /// Clip Studio Paint each on a new layer of its own.
    pub(crate) fn effect_lines(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let (a, b) = ([from.x, from.y], [to.x, to.y]);
        if (b[0] - a[0]).hypot(b[1] - a[1]) < 4.0 {
            return;
        }
        let speed = sub.mode == SPEED_LINES;
        let lines = manga::EffectLines {
            speed,
            a,
            b,
            count: if speed { 90 } else { 120 },
            line: sub.tip.size.max(0.5),
            seed: (a[0].to_bits() ^ b[1].to_bits().rotate_left(7)) | 1,
            rgb: self.drawing_color(),
        };
        let name = if speed {
            tr!("speed-lines")
        } else {
            tr!("focus-lines")
        };
        // An effect line layer: the Object tool moves its ends later, and
        // Tool Property changes its lines.
        self.spawn_drawn_layer(
            cx,
            &name,
            manga::EFFECT,
            lines.draw(width, height),
            vec![(manga::EFFECT_SPEC, Value::from(lines.encode()))],
        );
    }

    /// Closes the corners clicked out with a corner-laying sub tool: a
    /// polyline or curve balloon, a polyline frame, or a polyline
    /// selection (combined as `op`).
    pub(crate) fn finish_corners(&mut self, op: crate::select::Op, cx: &mut AppletContext<'_>) {
        let points = std::mem::take(&mut self.polyline);
        let owner = self.polyline_for.take();
        if points.len() < 3 {
            return;
        }
        let sub = self.toolbox.current().clone();
        match owner {
            Some((crate::tools::Tool::Balloon, mode)) => {
                let outline = if mode == CURVE_BALLOON {
                    manga::smooth_closed(&points, 10)
                } else {
                    points
                };
                self.add_balloon_shapes(cx, vec![Shape::Drawn(outline)], &sub, &tr!("balloon"));
            }
            Some((crate::tools::Tool::Frame, _)) => {
                self.add_panel(cx, points, &tr!("polyline-frame"));
            }
            _ => {
                self.polyline = points;
                self.finish_polyline(op);
            }
        }
    }

    /// Adds `panel` to the frame border layer (a new one when there is
    /// none) as one undo step called `label`.
    fn add_panel(&mut self, cx: &mut AppletContext<'_>, panel: Panel, label: &str) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            return;
        };
        match self.layer_of_class(&cx.world.arena, manga::FRAME) {
            Some(layer) => {
                let Some((mut panels, border, _)) = Self::frame_of(&cx.world.arena, layer) else {
                    return;
                };
                panels.push(panel);
                let pixels = manga::render_frames(&panels, width, height, border, paper);
                self.redraw_layer(
                    cx,
                    layer,
                    label,
                    manga::PANELS,
                    manga::encode_panels(&panels),
                    pixels,
                );
            }
            None => {
                let panels = vec![panel];
                let pixels = manga::render_frames(&panels, width, height, 6.0, paper);
                self.spawn_drawn_layer(
                    cx,
                    &tr!("frame"),
                    manga::FRAME,
                    pixels,
                    vec![
                        (manga::PANELS, Value::from(manga::encode_panels(&panels))),
                        (manga::BORDER, Value::from(6.0)),
                        (manga::GUTTER, Value::from(20.0)),
                    ],
                );
            }
        }
    }

    /// Whether the current layer is one the Object tool edits as comic
    /// parts (a frame border or effect line layer).
    pub(crate) fn on_comic_layer(&self, arena: &sue_core::Arena) -> bool {
        self.layer()
            .and_then(|layer| arena.get_by_id(layer).ok())
            .is_some_and(|entity| matches!(entity.class(), manga::FRAME | manga::EFFECT))
    }

    /// The Object tool pressed at `p` on a frame border or effect line
    /// layer: a panel's corner or the panel under it to move, or effect
    /// lines' ends. `reach` is how near a handle counts (picture pixels).
    pub(crate) fn comic_object_at(
        &self,
        p: Pos2,
        reach: f32,
        arena: &sue_core::Arena,
    ) -> Option<crate::studio::Gesture> {
        let layer = self.layer()?;
        let entity = arena.get_by_id(layer).ok()?;
        let at = [p.x, p.y];
        let near = |q: [f32; 2]| (q[0] - at[0]).hypot(q[1] - at[1]) <= reach;
        let edit = if entity.class() == manga::FRAME {
            let (panels, _, _) = Self::frame_of(arena, layer)?;
            // A corner first, then the panel the press is inside.
            let (panel, corner) = panels
                .iter()
                .enumerate()
                .find_map(|(panel, points)| {
                    points
                        .iter()
                        .position(|q| near(*q))
                        .map(|corner| (panel, Some(corner)))
                })
                .or_else(|| {
                    panels
                        .iter()
                        .position(|points| manga::signed_distance(points, at) <= 0.0)
                        .map(|panel| (panel, None))
                })?;
            ComicEdit::Panel {
                layer,
                now: panels.clone(),
                base: panels,
                panel,
                corner,
                start: at,
            }
        } else {
            let lines = Self::effect_of(arena, layer)?;
            let handle = if near(lines.a) {
                0
            } else if near(lines.b) {
                1
            } else {
                2
            };
            ComicEdit::Effect {
                layer,
                base: lines,
                now: lines,
                handle,
                start: at,
            }
        };
        Some(crate::studio::Gesture::Comic(Box::new(edit)))
    }

    /// The Object tool let go: the moved panel or effect lines kept as one
    /// undo step.
    pub(crate) fn finish_comic_edit(&mut self, edit: ComicEdit, cx: &mut AppletContext<'_>) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            return;
        };
        match edit {
            ComicEdit::Panel {
                layer, base, now, ..
            } => {
                if now == base {
                    return;
                }
                let Some((_, border, _)) = Self::frame_of(&cx.world.arena, layer) else {
                    return;
                };
                let pixels = manga::render_frames(&now, width, height, border, paper);
                self.redraw_layer(
                    cx,
                    layer,
                    &tr!("edit-frame"),
                    manga::PANELS,
                    manga::encode_panels(&now),
                    pixels,
                );
            }
            ComicEdit::Effect {
                layer, base, now, ..
            } => {
                if now != base {
                    self.redraw_effect(cx, layer, &now);
                }
            }
        }
    }

    /// An effect line layer's settings.
    pub(crate) fn effect_of(
        arena: &sue_core::Arena,
        layer: EntityId,
    ) -> Option<manga::EffectLines> {
        arena
            .get_by_id(layer)
            .ok()
            .filter(|entity| entity.class() == manga::EFFECT)?
            .property(manga::EFFECT_SPEC)
            .and_then(Value::as_str)
            .and_then(manga::EffectLines::decode)
    }

    /// Draws effect line layer `layer` again as `lines`, as one undo step.
    pub(crate) fn redraw_effect(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        lines: &manga::EffectLines,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let pixels = lines.draw(width, height);
        self.redraw_layer(
            cx,
            layer,
            &tr!("edit-effect-lines"),
            manga::EFFECT_SPEC,
            lines.encode(),
            pixels,
        );
    }

    /// Layer, New Layer, Balloon: an empty balloon layer on top.
    pub(crate) fn add_balloon_layer(&mut self, cx: &mut AppletContext<'_>) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        self.spawn_drawn_layer(
            cx,
            &tr!("balloon"),
            manga::BALLOON,
            vec![0; (width * height * 4) as usize],
            vec![
                (manga::SHAPES, Value::from(String::new())),
                (manga::LINE, Value::from(3.0)),
            ],
        );
        self.toolbox.pick(crate::tools::Tool::Balloon);
    }
}
