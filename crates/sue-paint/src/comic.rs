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
        let edit = Edit::new(format!("New {name}")).then(
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
            cx.fail("Open a page first");
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
            "Frame",
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
                cx.fail("Make frame borders first: Layer, New Layer, Frame Border Folder");
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
                cx.notify("Drag across a panel to divide it");
                return;
            };
            let pixels = manga::render_frames(&divided, width, height, border, paper);
            self.redraw_layer(
                cx,
                layer,
                "Divide Frame",
                manga::PANELS,
                manga::encode_panels(&divided),
                pixels,
            );
            return;
        }
        if (b[0] - a[0]).abs() < 4.0 || (b[1] - a[1]).abs() < 4.0 {
            return;
        }
        let panel = manga::rectangle(a, b);
        match existing {
            Some(layer) => {
                let Some((mut panels, border, _)) = Self::frame_of(&cx.world.arena, layer) else {
                    return;
                };
                panels.push(panel);
                let pixels = manga::render_frames(&panels, width, height, border, paper);
                self.redraw_layer(
                    cx,
                    layer,
                    "Rectangle Frame",
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
                    "Frame",
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

    /// A Balloon tool drag: a new ellipse balloon, or a tail from the
    /// balloon under `from` to `to`.
    pub(crate) fn balloon_drag(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let line = sub.tip.size.max(1.0);
        let existing = self.layer_of_class(&cx.world.arena, manga::BALLOON);
        let shapes = existing
            .and_then(|layer| cx.world.arena.get_by_id(layer).ok())
            .and_then(|entity| {
                entity
                    .property(manga::SHAPES)
                    .and_then(Value::as_str)
                    .map(manga::decode_shapes)
            })
            .unwrap_or_default();
        let (a, b) = ([from.x, from.y], [to.x, to.y]);
        let new = if sub.mode == 1 {
            // A tail about a third as wide as the balloon it leaves.
            let Some(balloon) = shapes.iter().rev().find_map(|shape| match shape {
                Shape::Ellipse([x0, y0, x1, y1])
                    if a[0] > *x0 && a[0] < *x1 && a[1] > *y0 && a[1] < *y1 =>
                {
                    Some((x1 - x0).min(y1 - y0))
                }
                _ => None,
            }) else {
                cx.fail("Start the tail inside a balloon");
                return;
            };
            manga::tail(a, b, (balloon * 0.35).max(8.0))
        } else {
            if (b[0] - a[0]).abs() < 4.0 || (b[1] - a[1]).abs() < 4.0 {
                return;
            }
            Shape::Ellipse([
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[0].max(b[0]),
                a[1].max(b[1]),
            ])
        };
        let mut shapes = shapes;
        shapes.push(new);
        let pixels = manga::render_balloons(&shapes, width, height, line);
        let text = manga::encode_shapes(&shapes);
        match existing {
            Some(layer) => {
                let label = if sub.mode == 1 {
                    "Balloon Tail"
                } else {
                    "Balloon"
                };
                self.redraw_layer(cx, layer, label, manga::SHAPES, text, pixels);
            }
            None => {
                self.spawn_drawn_layer(
                    cx,
                    "Balloon",
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

    /// Layer, New Layer, Balloon: an empty balloon layer on top.
    pub(crate) fn add_balloon_layer(&mut self, cx: &mut AppletContext<'_>) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        self.spawn_drawn_layer(
            cx,
            "Balloon",
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
