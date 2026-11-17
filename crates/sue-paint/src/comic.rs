//! Sue Paint's side of manga (Clip Studio Paint's frame borders and
//! balloons): frame border layers made from a layout, panels drawn as
//! rectangles or divided by dragging a line, and balloon layers with
//! balloons and tails. Every edit is one undo step that keeps the shapes
//! and the drawing together.

use egui::Pos2;
use sue_core::txn::{ReplaceComponent, SetProperty, SpawnEntity};
use sue_core::{Entity, EntityId, PixelFormat, RasterTile2D, Relation, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::AppletContext;

use crate::doc;
use crate::manga::{self, Panel, Shape};
use crate::studio::SuePaint;
use crate::tools::SubTool;

type Edit = sue_core::txn::Batch<sue_core::Arena>;

/// The Balloon tool's sub tools by mode (the constants live with the
/// tools in the picture engine).
pub(crate) use crate::tools::{
    BALLOON_PEN, CLOUD_BALLOON, CURVE_BALLOON, ELLIPSE_BALLOON, FLASH_BALLOON, ROUNDED_BALLOON,
    THOUGHT_TAIL,
};

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

/// A balloon layer's line color, `#rrggbb` (`Text`). Saved data: never
/// renamed without a migration. A layer without it has black lines.
pub(crate) const BALLOON_LINE_COLOR: &str = "balloon.line_color";
/// A balloon layer's fill color, `#rrggbb` (`Text`). Without it, white.
pub(crate) const BALLOON_FILL_COLOR: &str = "balloon.fill_color";

/// The colors a balloon layer draws its balloons in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BalloonStyle {
    pub line: [u8; 3],
    pub fill: [u8; 3],
}

impl BalloonStyle {
    /// Balloon layer `entity`'s colors (black lines on white where it
    /// names none, as balloons were before they had colors).
    pub(crate) fn of(entity: &Entity) -> Self {
        let color = |key: &str, default: [u8; 3]| {
            entity
                .property(key)
                .and_then(Value::as_str)
                .and_then(parse_hex)
                .unwrap_or(default)
        };
        Self {
            line: color(BALLOON_LINE_COLOR, [0, 0, 0]),
            fill: color(BALLOON_FILL_COLOR, [255, 255, 255]),
        }
    }

    /// Colors balloons drawn black on white (`manga::render_balloons`):
    /// each pixel goes from the line color to the fill color as it goes
    /// from black to white, its coverage kept.
    pub(crate) fn tint(self, pixels: &mut [u8]) {
        if self.line == [0, 0, 0] && self.fill == [255, 255, 255] {
            return;
        }
        for pixel in pixels.as_chunks_mut::<4>().0 {
            if pixel[3] == 0 {
                continue;
            }
            let white = f32::from(pixel[0]) / 255.0;
            for ((value, from), to) in pixel.iter_mut().zip(self.line).zip(self.fill) {
                let (from, to) = (f32::from(from), f32::from(to));
                *value = (from + (to - from) * white).round() as u8;
            }
        }
    }
}

/// `rgb` as `#rrggbb`.
fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// `#rrggbb` read back.
fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// How near, in screen points, a frame's corner snaps to the default
/// border.
const FRAME_SNAP: f32 = 12.0;
/// How strongly a picked panel is tinted with the selection color, 0 to 1:
/// enough to see which panel is picked, little enough to see the drawing.
const SELECTION_TINT: f32 = 0.3;

/// Divide Frame Border Equally: panel `panel` of frame layer `layer` cut
/// into `across` by `down` panels, `gap_x` and `gap_y` apart (pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DivideSpec {
    pub layer: EntityId,
    pub panel: usize,
    pub across: f64,
    pub down: f64,
    pub gap_x: f64,
    pub gap_y: f64,
}

/// Divide Frame Border Equally's settings.
pub(crate) fn divide_frame_ui(
    ui: &mut egui::Ui,
    spec: &mut DivideSpec,
    theme: &sue_wm::Theme,
) -> crate::dialogs::Outcome {
    let px = tr!("px");
    sue_wm::ui::form_grid(ui, "sue-paint-divide-frame", theme, |form| {
        form.row(&tr!("panels-across"), |ui| {
            sue_wm::ui::number_field("sue-paint-divide-across", &mut spec.across)
                .range(1.0..=MOST_PANELS)
                .show(ui, theme);
        });
        form.row(&tr!("panels-down"), |ui| {
            sue_wm::ui::number_field("sue-paint-divide-down", &mut spec.down)
                .range(1.0..=MOST_PANELS)
                .show(ui, theme);
        });
        form.row(&tr!("gutter-across"), |ui| {
            sue_wm::ui::number_field("sue-paint-divide-gap-x", &mut spec.gap_x)
                .range(0.0..=MOST_GUTTER)
                .suffix(&px)
                .show(ui, theme);
        });
        form.row(&tr!("gutter-down"), |ui| {
            sue_wm::ui::number_field("sue-paint-divide-gap-y", &mut spec.gap_y)
                .range(0.0..=MOST_GUTTER)
                .suffix(&px)
                .show(ui, theme);
        });
    });
    ui.add_space(theme.space(sue_wm::ui::Space::S4));
    let mut outcome = crate::dialogs::Outcome::Open;
    crate::dialogs::button_row(ui, |ui| {
        if sue_wm::ui::button("sue-paint-divide-ok", &tr!("ok"))
            .primary()
            .show(ui, theme)
            .clicked()
        {
            outcome = crate::dialogs::Outcome::Ok;
        }
        if sue_wm::ui::button("sue-paint-divide-cancel", &tr!("cancel"))
            .show(ui, theme)
            .clicked()
        {
            outcome = crate::dialogs::Outcome::Cancel;
        }
    });
    outcome
}

/// The most panels Divide Frame Border Equally makes each way.
const MOST_PANELS: f64 = 12.0;
/// The widest gutter it leaves, in pixels.
const MOST_GUTTER: f64 = 500.0;

/// The box round `panel`: `[left, top, right, bottom]`.
fn bounds(panel: &Panel) -> [f32; 4] {
    panel.iter().fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[x0, y0, x1, y1], p| [x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])],
    )
}

/// `pieces` each cut into `count` equal parts along `axis` (0 across, 1
/// down), `gap` apart, the earlier part first.
fn cut_equally(pieces: Vec<Panel>, count: usize, axis: usize, gap: f32) -> Vec<Panel> {
    let mut out = Vec::new();
    for piece in pieces {
        let [x0, y0, x1, y1] = bounds(&piece);
        let (start, length) = if axis == 0 {
            (x0, x1 - x0)
        } else {
            (y0, y1 - y0)
        };
        let cell = (length - gap * (count as f32 - 1.0)) / count as f32;
        if count < 2 || cell < 1.0 {
            out.push(piece);
            continue;
        }
        let mut done = Vec::new();
        let mut rest = piece;
        for step in 1..count {
            let at = start + step as f32 * cell + (step as f32 - 0.5) * gap;
            let (a, b) = if axis == 0 {
                ([at, y0 - 1.0], [at, y1 + 1.0])
            } else {
                ([x0 - 1.0, at], [x1 + 1.0, at])
            };
            let Some(mut two) = manga::divide(std::slice::from_ref(&rest), a, b, gap) else {
                break;
            };
            two.sort_by(|p, q| bounds(p)[axis].total_cmp(&bounds(q)[axis]));
            let Some(later) = two.pop() else {
                break;
            };
            done.extend(two);
            rest = later;
        }
        done.push(rest);
        out.extend(done);
    }
    out
}

/// `panel` cut into `across` columns and `down` rows of equal size,
/// `gap` (across, down) apart, in reading order: row by row, each left
/// to right.
pub(crate) fn divide_equally(
    panel: &Panel,
    across: usize,
    down: usize,
    gap: [f32; 2],
) -> Vec<Panel> {
    let rows = cut_equally(vec![panel.clone()], down, 1, gap[1]);
    cut_equally(rows, across, 0, gap[0])
}

/// `panels` with the panels at `picked` (two or more) made one: the
/// smallest convex panel holding them all, where the first of them was.
/// The panels after and the new one's index; `None` if fewer than two
/// of them exist.
pub(crate) fn combine_panels(panels: &[Panel], picked: &[usize]) -> Option<(Vec<Panel>, usize)> {
    if picked.iter().filter(|index| **index < panels.len()).count() < 2 {
        return None;
    }
    let first = *picked.iter().min()?;
    let mut points: Vec<[f32; 2]> = picked
        .iter()
        .filter_map(|index| panels.get(*index))
        .flatten()
        .copied()
        .collect();
    // Andrew's monotone chain: the convex hull, lower half then upper.
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    let turn = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f32; 2]> = Vec::new();
    for upper in [false, true] {
        let start = hull.len();
        let mut walk = |point: [f32; 2]| {
            while hull.len() >= start + 2
                && turn(hull[hull.len() - 2], hull[hull.len() - 1], point) <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        };
        if upper {
            points.iter().rev().for_each(|point| walk(*point));
        } else {
            points.iter().for_each(|point| walk(*point));
        }
        hull.pop();
    }
    if hull.len() < 3 {
        return None;
    }
    let mut out: Vec<Panel> = Vec::new();
    let mut at = 0;
    for (index, panel) in panels.iter().enumerate() {
        if index == first {
            at = out.len();
            out.push(hull.clone());
        } else if !picked.contains(&index) {
            out.push(panel.clone());
        }
    }
    Some((out, at))
}

/// `true` when `panel` turns the same way at every corner.
fn is_convex(panel: &Panel) -> bool {
    let count = panel.len();
    let mut sign = 0.0_f32;
    for index in 0..count {
        let (a, b, c) = (
            panel[index],
            panel[(index + 1) % count],
            panel[(index + 2) % count],
        );
        let turn = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if turn.abs() < f32::EPSILON {
            continue;
        }
        if sign != 0.0 && turn.signum() != sign {
            return false;
        }
        sign = turn.signum();
    }
    count >= 3
}

/// `p` moved onto the edges of `border` (`[left, top, right, bottom]`)
/// that are within `reach` of it, each axis on its own.
pub(crate) fn snap_to_border(p: [f32; 2], border: [f32; 4], reach: f32) -> [f32; 2] {
    let snap = |value: f32, edges: [f32; 2]| {
        edges
            .into_iter()
            .filter(|edge| (edge - value).abs() <= reach)
            .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
            .unwrap_or(value)
    };
    [
        snap(p[0], [border[0], border[2]]),
        snap(p[1], [border[1], border[3]]),
    ]
}

/// A folder made by the Frame Border tool for one frame (`Bool`): its
/// mask follows the panels of the frame border layer in it. Saved data.
pub(crate) const FRAME_FOLDER: &str = "frame.folder";

/// `true` if `folder` is a frame folder ([`FRAME_FOLDER`]).
pub(crate) fn is_frame_folder(arena: &sue_core::Arena, folder: EntityId) -> bool {
    arena.get_by_id(folder).is_ok_and(|entity| {
        entity
            .property(FRAME_FOLDER)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })
}

/// How much of each pixel of a `size` picture the panels cover, 0 to
/// 255: a frame folder's mask.
fn panel_cover(panels: &[Panel], [width, height]: [u32; 2]) -> Vec<u8> {
    let mut out = vec![0_u8; (width * height) as usize];
    for panel in panels {
        let [x0, y0, x1, y1] = bounds(panel);
        let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
        for y in clamp(y0 - 1.0, height)..clamp(y1 + 2.0, height) {
            for x in clamp(x0 - 1.0, width)..clamp(x1 + 2.0, width) {
                let d = manga::outside(panel, [x as f32 + 0.5, y as f32 + 0.5]);
                let cover = ((0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u8;
                let at = &mut out[(y * width + x) as usize];
                *at = (*at).max(cover);
            }
        }
    }
    out
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
    pub(crate) fn page(&self, arena: &sue_core::Arena) -> Option<(EntityId, [u32; 2], [u8; 3])> {
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
        // Where every new layer goes: in the picked folder, or right
        // above the picked layer in its folder.
        let edit = Edit::new(tr!("new", name = name)).then(doc::landed(
            &cx.world.arena,
            picture,
            self.layer(),
            layer,
        ));
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
        let mut edit = Edit::new(label);
        // A frame folder's mask shows its panels as they are now.
        if key == manga::PANELS
            && let Some(mask) = self
                .picture()
                .and_then(|_| doc::folder_of(&cx.world.arena, layer))
                .filter(|folder| is_frame_folder(&cx.world.arena, *folder))
                .and_then(|folder| doc::mask_of(&cx.world.arena, folder))
            && let Ok(cover) = RasterTile2D::from_bytes(
                width,
                height,
                PixelFormat::Gray8,
                panel_cover(&manga::decode_panels(&text), [width, height]),
            )
        {
            edit = edit.then(ReplaceComponent::new(mask, cover, label));
        }
        let edit = edit
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
        let existing = self.frame_layer_near(&cx.world.arena);
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
            let Some((panels, border, _)) = Self::frame_of(&cx.world.arena, layer) else {
                return;
            };
            // The sub tool's gutters: a cut running down leaves the
            // vertical gutter between the panels side by side, a cut
            // across the horizontal one.
            let gutter = if (b[0] - a[0]).abs() < (b[1] - a[1]).abs() {
                sub.gutter[0]
            } else {
                sub.gutter[1]
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
        // The corners snap to the page's default border.
        let reach = self.frame_reach();
        let a = self.frame_snap(&cx.world.arena, a, reach);
        let b = self.frame_snap(&cx.world.arena, b, reach);
        if (b[0] - a[0]).abs() < 4.0 || (b[1] - a[1]).abs() < 4.0 {
            return;
        }
        self.add_panel(cx, manga::rectangle(a, b), sub, &tr!("rectangle-frame"));
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
        let under = self
            .balloon_at(&cx.world.arena, a)
            .map(|(_, balloon)| balloon);
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
        self.add_balloon_shapes(cx, new, sub, &label, Some(a));
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
        self.add_balloon_shapes(cx, vec![Shape::Drawn(outline)], sub, &tr!("balloon"), None);
    }

    /// The balloon layer `layer`'s shapes.
    fn shapes_of(arena: &sue_core::Arena, layer: EntityId) -> Vec<Shape> {
        arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| entity.property(manga::SHAPES).and_then(Value::as_str))
            .map(manga::decode_shapes)
            .unwrap_or_default()
    }

    /// The balloon layers of the current picture, the current layer first
    /// when it is one, then from the top down.
    fn balloon_layers(&self, arena: &sue_core::Arena) -> Vec<EntityId> {
        let Some(picture) = self.picture() else {
            return Vec::new();
        };
        let is = |id: EntityId| {
            arena
                .get_by_id(id)
                .is_ok_and(|entity| entity.class() == manga::BALLOON)
        };
        let current = self.layer().filter(|id| is(*id));
        current
            .into_iter()
            .chain(
                doc::layers(arena, picture)
                    .into_iter()
                    .rev()
                    .map(|info| info.id)
                    .filter(|id| is(*id) && Some(*id) != current),
            )
            .collect()
    }

    /// The balloon at `p` (picture pixels), on whichever balloon layer
    /// holds it, the current one first: its layer and its box.
    fn balloon_at(&self, arena: &sue_core::Arena, p: [f32; 2]) -> Option<(EntityId, [f32; 4])> {
        self.balloon_layers(arena).into_iter().find_map(|layer| {
            Self::shapes_of(arena, layer)
                .iter()
                .rev()
                .find_map(|shape| {
                    shape.balloon().filter(|[x0, y0, x1, y1]| {
                        p[0] > *x0 && p[0] < *x1 && p[1] > *y0 && p[1] < *y1
                    })
                })
                .map(|balloon| (layer, balloon))
        })
    }

    /// A balloon layer's line width (3 pixels where it names none).
    fn line_of(arena: &sue_core::Arena, layer: EntityId) -> f32 {
        arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| entity.property(manga::LINE))
            .and_then(Value::as_f64)
            .map_or(3.0, |line| line as f32)
    }

    /// Adds `new` shapes to a balloon layer as one undo step called
    /// `label`. A tail goes to the layer of the balloon it leaves. A
    /// balloon goes to the current layer when it is a balloon layer
    /// drawn the way the sub tool says (How to add: Add to selected
    /// layer), else to a new balloon layer with the sub tool's edge and
    /// colors, so every balloon looks as its tool was set.
    fn add_balloon_shapes(
        &mut self,
        cx: &mut AppletContext<'_>,
        new: Vec<Shape>,
        sub: &SubTool,
        label: &str,
        from: Option<[f32; 2]>,
    ) {
        let Some((_, [width, height], _)) = self.page(&cx.world.arena) else {
            return;
        };
        let arena = &cx.world.arena;
        let tail = matches!(sub.mode, 1 | THOUGHT_TAIL);
        let (existing, style, line) = if tail {
            let layer = from
                .and_then(|p| self.balloon_at(arena, p))
                .map(|(layer, _)| layer)
                .or_else(|| self.layer_of_class(arena, manga::BALLOON));
            let Some(layer) = layer else {
                return;
            };
            let style = arena
                .get_by_id(layer)
                .map_or_else(|_| self.balloon_style(sub), BalloonStyle::of);
            (Some(layer), style, Self::line_of(arena, layer))
        } else {
            let style = self.balloon_style(sub);
            let line = if sub.edge { sub.tip.size.max(0.5) } else { 0.0 };
            let same = |layer: &EntityId| {
                arena.get_by_id(*layer).is_ok_and(|entity| {
                    BalloonStyle::of(entity) == style
                        && (Self::line_of(arena, *layer) - line).abs() < 0.01
                })
            };
            let existing = (!sub.new_layer)
                .then(|| self.layer())
                .flatten()
                .filter(|layer| {
                    arena
                        .get_by_id(*layer)
                        .is_ok_and(|entity| entity.class() == manga::BALLOON)
                })
                .filter(same);
            (existing, style, line)
        };
        let mut shapes = existing.map_or_else(Vec::new, |layer| Self::shapes_of(arena, layer));
        shapes.extend(new);
        let mut pixels = manga::render_balloons(&shapes, width, height, line);
        style.tint(&mut pixels);
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
                        (BALLOON_LINE_COLOR, Value::from(hex(style.line))),
                        (BALLOON_FILL_COLOR, Value::from(hex(style.fill))),
                    ],
                );
            }
        }
    }

    /// The colors a new balloon of `sub` is drawn in: its Line color and
    /// Fill color, the drawing color, the other of the main and sub
    /// colors, or its own (by default the line in the drawing color and
    /// the inside in the other: black on white, as Clip Studio Paint's
    /// main and sub colors give).
    fn balloon_style(&self, sub: &SubTool) -> BalloonStyle {
        let (drawing, other) = if self.sub_active {
            (self.sub, self.main)
        } else {
            (self.main, self.sub)
        };
        BalloonStyle {
            line: sub.line_color.rgb(drawing, other),
            fill: sub.fill_color.rgb(drawing, other),
        }
    }

    /// A polyline or curve balloon closed with the corners `points`.
    pub(crate) fn balloon_corners(
        &mut self,
        points: Vec<[f32; 2]>,
        mode: u8,
        sub: SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let outline = if mode == CURVE_BALLOON {
            manga::smooth_closed(&points, 10)
        } else {
            points
        };
        self.add_balloon_shapes(cx, vec![Shape::Drawn(outline)], &sub, &tr!("balloon"), None);
    }

    /// A polyline frame closed with the corners `points`.
    pub(crate) fn frame_corners(
        &mut self,
        points: Vec<[f32; 2]>,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        self.add_panel(cx, points, sub, &tr!("polyline-frame"));
    }

    /// The Frame pen: a frame round the outline drawn, smoothed a little.
    pub(crate) fn frame_drawn(
        &mut self,
        points: Vec<[f32; 2]>,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let outline = crate::correct::simplified_points(&points, 1.5);
        if outline.len() < 3 {
            return;
        }
        self.add_panel(cx, outline, sub, &tr!("frame-pen"));
    }

    /// Adds `panel` as one undo step called `label`: in a frame folder of
    /// its own when the sub tool's How to add says so (Clip Studio
    /// Paint's Create new folder), else to the frame border layer being
    /// worked on (a new one when there is none).
    fn add_panel(&mut self, cx: &mut AppletContext<'_>, panel: Panel, sub: &SubTool, label: &str) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            return;
        };
        let border = if sub.edge { sub.tip.size.max(0.5) } else { 0.0 };
        // How to add: Create new frame folder makes a folder for every
        // frame, one after another, even while the last one's frame
        // border layer is picked. Only a frame border layer of a layout
        // (New Frame Border Folder: a page of panels in one layer, not in
        // a folder this tool made) takes the panel in, as a page of
        // panels does. Add to frame folder always puts the panel with the
        // frame border layer being worked on.
        let arena = &cx.world.arena;
        let on_layout = self.layer().is_some_and(|layer| {
            arena
                .get_by_id(layer)
                .is_ok_and(|entity| entity.class() == manga::FRAME)
                && !doc::folder_of(arena, layer)
                    .is_some_and(|folder| is_frame_folder(arena, folder))
        });
        if sub.new_layer && !on_layout {
            self.add_frame_folder(cx, panel, sub, border, label);
            return;
        }
        match self.frame_layer_near(&cx.world.arena) {
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
                let pixels = manga::render_frames(&panels, width, height, border, paper);
                self.spawn_drawn_layer(
                    cx,
                    &tr!("frame"),
                    manga::FRAME,
                    pixels,
                    vec![
                        (manga::PANELS, Value::from(manga::encode_panels(&panels))),
                        (manga::BORDER, Value::from(f64::from(border))),
                        (manga::GUTTER, Value::from(f64::from(sub.gutter[0]))),
                    ],
                );
            }
        }
    }

    /// The frame border layer the Frame Border tool works on: the current
    /// layer when it is one, else the one in the current layer's frame
    /// folder, else the topmost.
    fn frame_layer_near(&self, arena: &sue_core::Arena) -> Option<EntityId> {
        let picture = self.picture()?;
        let is_frame = |id: &EntityId| {
            arena
                .get_by_id(*id)
                .is_ok_and(|entity| entity.class() == manga::FRAME)
        };
        let current = self.layer()?;
        if is_frame(&current) {
            return Some(current);
        }
        let folder = if doc::is_folder(arena, current) {
            Some(current)
        } else {
            doc::folder_of(arena, current)
        };
        folder
            .filter(|folder| is_frame_folder(arena, *folder))
            .and_then(|folder| {
                doc::members(arena, picture, folder)
                    .into_iter()
                    .find(|id| is_frame(id))
            })
            .or_else(|| self.layer_of_class(arena, manga::FRAME))
    }

    /// A frame folder holding `panel` (Clip Studio Paint's frame border
    /// folder): a folder masked to the panel, holding a white background,
    /// a layer to draw on and the frame border layer that inks its
    /// border, made where a new layer goes as one undo step. The frame
    /// border layer becomes the current layer, so the Object tool and
    /// the division work on it at once.
    fn add_frame_folder(
        &mut self,
        cx: &mut AppletContext<'_>,
        panel: Panel,
        sub: &SubTool,
        border: f32,
        label: &str,
    ) {
        let arena = &cx.world.arena;
        let Some((picture, [width, height], paper)) = self.page(arena) else {
            return;
        };
        // Beside the frame folder being worked in (never inside it), else
        // where a new layer goes.
        let current = self.layer();
        let in_frame = current.and_then(|layer| {
            std::iter::once(layer)
                .chain(doc::folders_around(arena, layer))
                .find(|id| is_frame_folder(arena, *id))
        });
        let (keys, outer) = match in_frame {
            Some(folder) => (
                doc::keys_above(arena, picture, Some(folder), 4),
                doc::folder_of(arena, folder),
            ),
            None => doc::landing(arena, picture, current, 4),
        };
        let [back_key, draw_key, frame_key, folder_key] =
            <[_; 4]>::try_from(keys).ok().unwrap_or_default();
        let count = doc::layers(arena, picture)
            .iter()
            .filter(|info| is_frame_folder(arena, info.id))
            .count();
        let folder = Entity::empty(
            tr!("frame-folder-name", value = count + 1),
            sue_model::class::FOLDER,
        )
        .with_property(doc::OPEN, true)
        .with_property(doc::MASK_ON, true)
        .with_property(FRAME_FOLDER, true);
        let folder_id = folder.id();
        let Ok(mask_raster) = RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Gray8,
            panel_cover(std::slice::from_ref(&panel), [width, height]),
        ) else {
            return;
        };
        let mask = Entity::new(tr!("mask"), mask_raster).with_class(doc::MASK_CLASS);
        let mask_id = mask.id();
        let back = Entity::empty(tr!("frame-paper"), doc::FILL_LAYER).with_property(
            doc::FILL_COLOR,
            sue_core::Color::srgb8(paper[0], paper[1], paper[2]),
        );
        let back_id = back.id();
        let Ok((draw, draw_id)) =
            doc::add_raster_layer_at(arena, picture, draw_key, tr!("frame-drawing-layer"), None)
        else {
            return;
        };
        let panels = vec![panel];
        let Ok(frame_raster) = RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            manga::render_frames(&panels, width, height, border, paper),
        ) else {
            return;
        };
        let frame = Entity::new(tr!("frame"), frame_raster)
            .with_class(manga::FRAME)
            .with_property(manga::PANELS, Value::from(manga::encode_panels(&panels)))
            .with_property(manga::BORDER, Value::from(f64::from(border)))
            .with_property(manga::GUTTER, Value::from(f64::from(sub.gutter[0])));
        let frame_id = frame.id();
        let child = |id: EntityId, key| Relation::new(picture, sue_core::CHILD, id).ordered(key);
        let member = |id: EntityId| Relation::new(folder_id, doc::MEMBER, id);
        let mut spawn_folder = SpawnEntity::new(folder).with_relation(child(folder_id, folder_key));
        if let Some(outer) = outer {
            spawn_folder = spawn_folder.with_relation(Relation::new(outer, doc::MEMBER, folder_id));
        }
        let edit = Edit::new(label)
            .then(spawn_folder)
            .then(SpawnEntity::new(mask).with_relation(Relation::new(
                folder_id,
                doc::MASK,
                mask_id,
            )))
            .then(
                SpawnEntity::new(back)
                    .with_relation(child(back_id, back_key))
                    .with_relation(member(back_id)),
            )
            .then(draw.with_relation(member(draw_id)))
            .then(
                SpawnEntity::new(frame)
                    .with_relation(child(frame_id, frame_key))
                    .with_relation(member(frame_id)),
            );
        if !Self::apply(cx, edit) {
            return;
        }
        self.sync_docs(&cx.world.arena);
        if let Some(doc) = self.doc_mut() {
            doc.layer = Some(frame_id);
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
    /// The panel pressed becomes the one picked; with `add` (Shift) it is
    /// added to the panels picked, or taken out of them.
    pub(crate) fn comic_object_at(
        &mut self,
        p: Pos2,
        reach: f32,
        add: bool,
        arena: &sue_core::Arena,
    ) -> Option<crate::studio::Gesture> {
        let layer = self.layer()?;
        let entity = arena.get_by_id(layer).ok()?;
        let at = [p.x, p.y];
        let near = |q: [f32; 2]| (q[0] - at[0]).hypot(q[1] - at[1]) <= reach;
        let edit = if entity.class() == manga::FRAME {
            let (panels, _, _) = Self::frame_of(arena, layer)?;
            // A corner first, then the panel the press is inside.
            let found = panels
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
                });
            let Some((panel, corner)) = found else {
                if !add {
                    self.comic_picked = None;
                }
                return None;
            };
            self.pick_panel(layer, panel, add);
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

    /// Picks panel `panel` of frame layer `layer`; with `add`, adds it to
    /// the panels picked on that layer or takes it out of them.
    fn pick_panel(&mut self, layer: EntityId, panel: usize, add: bool) {
        match &mut self.comic_picked {
            Some((on, picked)) if add && *on == layer => {
                if let Some(at) = picked.iter().position(|known| *known == panel) {
                    picked.remove(at);
                } else {
                    picked.push(panel);
                }
            }
            _ => self.comic_picked = Some((layer, vec![panel])),
        }
    }

    /// The frame border layer to divide or combine on, and the panels
    /// picked on it that still exist, in order. A layer of one panel has
    /// that panel picked.
    pub(crate) fn picked_panels(&self, arena: &sue_core::Arena) -> Option<(EntityId, Vec<usize>)> {
        let layer = self.layer_of_class(arena, manga::FRAME)?;
        let (panels, _, _) = Self::frame_of(arena, layer)?;
        let mut picked: Vec<usize> = match &self.comic_picked {
            Some((on, picked)) if *on == layer => picked
                .iter()
                .copied()
                .filter(|panel| *panel < panels.len())
                .collect(),
            _ if panels.len() == 1 => vec![0],
            _ => Vec::new(),
        };
        picked.sort_unstable();
        picked.dedup();
        Some((layer, picked))
    }

    /// Layer, Ruler/Frame, Divide Frame Border Equally: asks how, for the
    /// one panel picked.
    pub(crate) fn ask_divide_frame(&mut self, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let Some((layer, picked)) = self.picked_panels(arena) else {
            cx.fail(tr!("make-frame-borders-first-layer-new"));
            return;
        };
        let [panel] = picked[..] else {
            cx.notify(tr!("pick-one-panel-to-divide"));
            return;
        };
        let gutter = Self::frame_of(arena, layer).map_or(20.0, |(_, _, gutter)| gutter);
        self.dialog = Some(crate::dialogs::Dialog::DivideFrame(DivideSpec {
            layer,
            panel,
            across: 2.0,
            down: 1.0,
            gap_x: f64::from(gutter),
            gap_y: f64::from(gutter * 2.0),
        }));
    }

    /// Divides the panel `spec` names as it says, as one undo step; the
    /// new panels are picked.
    pub(crate) fn divide_frame_ok(&mut self, cx: &mut AppletContext<'_>, spec: &DivideSpec) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            return;
        };
        let Some((mut panels, border, _)) = Self::frame_of(&cx.world.arena, spec.layer) else {
            return;
        };
        let Some(panel) = panels.get(spec.panel) else {
            return;
        };
        let pieces = divide_equally(
            panel,
            spec.across.round().max(1.0) as usize,
            spec.down.round().max(1.0) as usize,
            [spec.gap_x as f32, spec.gap_y as f32],
        );
        if pieces.len() < 2 {
            return;
        }
        let made = pieces.len();
        panels.splice(spec.panel..=spec.panel, pieces);
        let pixels = manga::render_frames(&panels, width, height, border, paper);
        self.redraw_layer(
            cx,
            spec.layer,
            &tr!("divide-frame-border-equally"),
            manga::PANELS,
            manga::encode_panels(&panels),
            pixels,
        );
        self.comic_picked = Some((spec.layer, (spec.panel..spec.panel + made).collect()));
    }

    /// Layer, Ruler/Frame, Combine Frames: the panels picked become one,
    /// the smallest convex panel holding them all, as one undo step.
    pub(crate) fn combine_frames(&mut self, cx: &mut AppletContext<'_>) {
        let Some((_, [width, height], paper)) = self.page(&cx.world.arena) else {
            return;
        };
        let Some((layer, picked)) = self.picked_panels(&cx.world.arena) else {
            return;
        };
        if picked.len() < 2 {
            cx.notify(tr!("pick-panels-to-combine"));
            return;
        }
        let Some((panels, border, _)) = Self::frame_of(&cx.world.arena, layer) else {
            return;
        };
        let Some((combined, at)) = combine_panels(&panels, &picked) else {
            return;
        };
        let pixels = manga::render_frames(&combined, width, height, border, paper);
        self.redraw_layer(
            cx,
            layer,
            &tr!("combine-frames"),
            manga::PANELS,
            manga::encode_panels(&combined),
            pixels,
        );
        self.comic_picked = Some((layer, vec![at]));
    }

    /// Where a Rectangle frame's corner lands: on the page's default
    /// border when it is within `reach` of it (picture pixels), as Clip
    /// Studio Paint's frames snap to it.
    fn frame_snap(&self, arena: &sue_core::Arena, p: [f32; 2], reach: f32) -> [f32; 2] {
        self.picture()
            .and_then(|picture| crate::story::borders(arena, picture))
            .map_or(p, |borders| snap_to_border(p, borders.inner, reach))
    }

    /// How near a frame corner snaps to the default border, in picture
    /// pixels at the current zoom.
    fn frame_reach(&self) -> f32 {
        FRAME_SNAP / self.doc().map_or(1.0, |doc| doc.view.zoom.max(0.01))
    }

    /// What the canvas shows over the page for comic work: the panels the
    /// Object tool picked, and where a Rectangle frame being dragged will
    /// land once it snaps to the default border.
    pub(crate) fn comic_overlay(
        &self,
        ui: &egui::Ui,
        viewport: egui::Rect,
        size: [u32; 2],
        cx: &AppletContext<'_>,
        theme: &sue_wm::Theme,
    ) {
        let Some(doc) = self.doc() else {
            return;
        };
        let view = doc.view;
        let screen = |p: [f32; 2]| view.to_screen(viewport, size, egui::pos2(p[0], p[1]));
        let painter = ui.painter().with_clip_rect(viewport);
        let stroke = egui::Stroke::new(theme.focus_stroke().width, theme.selection.to_opaque());
        let arena = &cx.world.arena;
        if self.toolbox.tool() == crate::tools::Tool::Operation
            && self.on_comic_layer(arena)
            && let Some((layer, picked)) = self.picked_panels(arena)
            && self
                .comic_picked
                .as_ref()
                .is_some_and(|(on, _)| *on == layer)
            && let Some((panels, _, _)) = Self::frame_of(arena, layer)
        {
            for panel in picked.iter().filter_map(|index| panels.get(*index)) {
                let points: Vec<egui::Pos2> = panel.iter().map(|p| screen(*p)).collect();
                // Tinted as a selection is; a panel that is not convex
                // (a polyline frame) is outlined only, as a fill could
                // spill out of it.
                if is_convex(panel) {
                    painter.add(egui::Shape::convex_polygon(
                        points.clone(),
                        theme.selection.gamma_multiply(SELECTION_TINT),
                        egui::Stroke::NONE,
                    ));
                }
                painter.add(egui::Shape::closed_line(points, stroke));
            }
        }
        if let Some(crate::studio::Gesture::Shape {
            tool: crate::tools::Tool::Frame,
            sub,
            from,
            to,
            ..
        }) = &self.gesture
            && sub.mode == 0
        {
            let reach = self.frame_reach();
            let a = self.frame_snap(arena, [from.x, from.y], reach);
            let b = self.frame_snap(arena, [to.x, to.y], reach);
            if a != [from.x, from.y] || b != [to.x, to.y] {
                painter.add(egui::Shape::closed_line(
                    manga::rectangle(a, b).into_iter().map(screen).collect(),
                    stroke,
                ));
            }
        }
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
        let balloon = self.toolbox.of(crate::tools::Tool::Balloon).clone();
        self.spawn_drawn_layer(
            cx,
            &tr!("balloon"),
            manga::BALLOON,
            vec![0; (width * height * 4) as usize],
            vec![
                (manga::SHAPES, Value::from(String::new())),
                (manga::LINE, Value::from(3.0)),
                (
                    BALLOON_LINE_COLOR,
                    Value::from(hex(self.balloon_style(&balloon).line)),
                ),
                (
                    BALLOON_FILL_COLOR,
                    Value::from(hex(self.balloon_style(&balloon).fill)),
                ),
            ],
        );
        self.toolbox.pick(crate::tools::Tool::Balloon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_divides_into_equal_panels_with_gutters_between() {
        let panel = manga::rectangle([0.0, 0.0], [220.0, 100.0]);
        let pieces = divide_equally(&panel, 3, 2, [20.0, 10.0]);
        assert_eq!(pieces.len(), 6);
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        // Reading order: the first row left to right, then the second.
        for (index, piece) in pieces.iter().enumerate() {
            let b = bounds(piece);
            let (column, row) = ((index % 3) as f32, (index / 3) as f32);
            assert!(near(b[0], column * 80.0), "{index}: {b:?}");
            assert!(near(b[2] - b[0], 60.0), "{index}: {b:?}");
            assert!(near(b[1], row * 55.0), "{index}: {b:?}");
            assert!(near(b[3] - b[1], 45.0), "{index}: {b:?}");
        }
        // One by one is the panel itself.
        assert_eq!(divide_equally(&panel, 1, 1, [20.0, 10.0]), vec![panel]);
    }

    #[test]
    fn panels_combine_into_the_one_that_holds_them() {
        let panels = vec![
            manga::rectangle([0.0, 0.0], [100.0, 50.0]),
            manga::rectangle([0.0, 60.0], [45.0, 120.0]),
            manga::rectangle([55.0, 60.0], [100.0, 120.0]),
        ];
        let (combined, at) = combine_panels(&panels, &[1, 2]).unwrap();
        assert_eq!(combined.len(), 2);
        assert_eq!(at, 1);
        assert_eq!(bounds(&combined[1]), [0.0, 60.0, 100.0, 120.0]);
        assert_eq!(combined[1].len(), 4, "a rectangle again: {:?}", combined[1]);
        assert_eq!(combined[0], panels[0]);
        // One panel alone combines with nothing.
        assert!(combine_panels(&panels, &[0]).is_none());
    }

    #[test]
    fn balloons_take_their_layer_colors_and_may_have_no_edge() {
        let shapes = vec![Shape::Ellipse([4.0, 4.0, 36.0, 28.0])];
        let style = BalloonStyle {
            line: [200, 10, 10],
            fill: [250, 240, 200],
        };
        let mut pixels = manga::render_balloons(&shapes, 40, 32, 3.0);
        style.tint(&mut pixels);
        let at = |x: usize, y: usize| {
            let index = (y * 40 + x) * 4;
            [
                pixels[index],
                pixels[index + 1],
                pixels[index + 2],
                pixels[index + 3],
            ]
        };
        assert_eq!(at(20, 16), [250, 240, 200, 255], "filled inside");
        assert_eq!(at(20, 2), [200, 10, 10, 255], "lined round the edge");
        assert_eq!(at(1, 1)[3], 0, "clear outside");
        // No edge: a line of 0 leaves the fill alone.
        let mut bare = manga::render_balloons(&shapes, 40, 32, 0.0);
        style.tint(&mut bare);
        assert!(
            bare.as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] == 255)
                .all(|pixel| pixel[..3] == [250, 240, 200]),
            "only fill"
        );
        // The colors round-trip through the layer's properties.
        assert_eq!(parse_hex(&hex([1, 128, 255])), Some([1, 128, 255]));
        assert_eq!(parse_hex("red"), None);
        let entity = Entity::empty("Balloon", manga::BALLOON)
            .with_property(BALLOON_LINE_COLOR, Value::from(hex(style.line)))
            .with_property(BALLOON_FILL_COLOR, Value::from(hex(style.fill)));
        assert_eq!(BalloonStyle::of(&entity), style);
        let old = Entity::empty("Balloon", manga::BALLOON);
        assert_eq!(
            BalloonStyle::of(&old),
            BalloonStyle {
                line: [0, 0, 0],
                fill: [255, 255, 255]
            }
        );
    }

    #[test]
    fn convex_panels_are_told_from_others() {
        assert!(is_convex(&manga::rectangle([0.0, 0.0], [10.0, 5.0])));
        let notch = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [5.0, 3.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ];
        assert!(!is_convex(&notch));
    }

    #[test]
    fn a_corner_snaps_to_the_default_border_within_reach() {
        let border = [50.0, 60.0, 450.0, 640.0];
        assert_eq!(snap_to_border([55.0, 300.0], border, 8.0), [50.0, 300.0]);
        assert_eq!(snap_to_border([445.0, 633.0], border, 8.0), [450.0, 640.0]);
        assert_eq!(snap_to_border([70.0, 300.0], border, 8.0), [70.0, 300.0]);
    }
}
