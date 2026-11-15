//! Sue Paint's transforms, as in Clip Studio Paint: Scale/Rotate (Ctrl+T),
//! Free Transform (Ctrl+Shift+T), Scale, Rotate, Skew, Distort,
//! Perspective and Mesh Transformation. The selected part of the current
//! layer (or all of it, or every layer of the current folder and of the
//! layers picked with it) goes in a box with handles:
//!
//! - a corner scales (keeping the shape unless Shift, or unless Keep
//!   aspect ratio is off), or in the modes that free the corners goes
//!   its own way; Ctrl on a corner distorts in any mode;
//! - an edge stretches, inside moves, outside turns around the rotation
//!   centre (Shift turns in steps of 15 degrees);
//! - the rotation centre is a handle of its own, and the bar puts it on
//!   any of the box's corners, edges or its middle;
//! - Mesh Transformation lays a lattice of a chosen number of points each
//!   way, each dragged on its own.
//!
//! The bar above the canvas holds what Clip Studio Paint keeps in the
//! Transform's Tool Property (the mode, the rotation centre, Keep aspect
//! ratio, Keep original image, the scale, angle and position as numbers)
//! with Reset, Cancel and OK. Enter keeps it as one undo step for every
//! layer it changed, Esc puts everything back.

use egui::{Align2, Id, Key, Modifiers, Pos2, Rect, Ui, Vec2, pos2};
use sue_core::{EntityId, RasterTile2D};
use sue_wm::icons::Icon;
use sue_wm::ui::{Elevation, Radius, Space, Tip};
use sue_wm::{AppletContext, ArenaEdit, Theme};

use crate::doc;
use crate::launchers::{Button, Slot};
use crate::studio::{Gesture, SuePaint};
use crate::transform::{self, Lattice, Mode, Quad};
use crate::view::View;

/// What a press in the transform box grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    /// A corner, 0 top left, then clockwise.
    Corner(usize),
    /// An edge: 0 top, 1 right, 2 bottom, 3 left.
    Edge(usize),
    /// The inside: moves it.
    Move,
    /// Outside the box: turns it around the rotation centre.
    Rotate,
    /// The rotation centre itself.
    Pivot,
    /// A point of Mesh Transformation's lattice, by its place in it.
    Point(usize),
}

/// Where the rotation centre sits: one of the box's nine points, or
/// wherever it was dragged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pivot {
    #[default]
    Middle,
    /// A point of the box: `(u, v)` in halves, 0 to 2 each way.
    Point(u8, u8),
    /// Dragged, in picture pixels.
    Free,
}

impl Pivot {
    /// The choices the bar lists, in reading order, then Free.
    const LIST: [Self; 10] = [
        Self::Point(0, 0),
        Self::Point(1, 0),
        Self::Point(2, 0),
        Self::Point(0, 1),
        Self::Middle,
        Self::Point(2, 1),
        Self::Point(0, 2),
        Self::Point(1, 2),
        Self::Point(2, 2),
        Self::Free,
    ];

    fn label(self) -> String {
        match self {
            Self::Middle | Self::Point(1, 1) => tr!("pivot-middle"),
            Self::Point(0, 0) => tr!("pivot-top-left"),
            Self::Point(1, 0) => tr!("pivot-top"),
            Self::Point(2, 0) => tr!("pivot-top-right"),
            Self::Point(0, 1) => tr!("pivot-left"),
            Self::Point(2, 1) => tr!("pivot-right"),
            Self::Point(0, 2) => tr!("pivot-bottom-left"),
            Self::Point(1, 2) => tr!("pivot-bottom"),
            Self::Point(..) => tr!("pivot-bottom-right"),
            Self::Free => tr!("pivot-free"),
        }
    }

    /// Where it is on `quad`, or `None` for a dragged one.
    fn on(self, quad: &Quad) -> Option<[f32; 2]> {
        match self {
            Self::Middle => Some(transform::middle(quad)),
            Self::Point(u, v) => Some(transform::on_quad(
                quad,
                [f32::from(u) / 2.0, f32::from(v) / 2.0],
            )),
            Self::Free => None,
        }
    }
}

/// What transforms keep from one use to the next (the Transform's Tool
/// Property in Clip Studio Paint), and Puppet warp's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TransformPrefs {
    /// A corner keeps the box's shape (Shift does the other).
    pub keep_aspect: bool,
    /// What is transformed stays where it was too (Keep original image).
    pub keep_original: bool,
    /// Mesh Transformation's points across and down.
    pub mesh: [usize; 2],
    /// Where a new transform puts its rotation centre.
    pub pivot: Pivot,
    /// Puppet warp's settings.
    pub puppet: crate::warp::PuppetSettings,
}

impl Default for TransformPrefs {
    fn default() -> Self {
        Self {
            keep_aspect: true,
            keep_original: false,
            mesh: [4, 4],
            pivot: Pivot::Middle,
            // A mesh square of 16 pixels: as fine as a bend needs, and
            // coarse enough to see the drawing through its lines.
            puppet: crate::warp::PuppetSettings {
                spacing: 16,
                ..crate::warp::PuppetSettings::default()
            },
        }
    }
}

/// One layer being transformed.
#[derive(Clone, Debug)]
pub(crate) struct Part {
    pub layer: EntityId,
    pub size: [u32; 2],
    pub before: Vec<u8>,
    pub rest: Vec<u8>,
    pub piece: Vec<u8>,
    /// The piece's rectangle in the layer's pixels.
    pub rect: [u32; 4],
    pub offset: [i64; 2],
}

impl Part {
    /// The piece's rectangle in picture pixels.
    fn picture_rect(&self) -> [f32; 4] {
        let [ox, oy] = self.offset.map(|value| value as f32);
        let [x0, y0, x1, y1] = self.rect.map(|value| value as f32);
        [x0 + ox, y0 + oy, x1 + ox, y1 + oy]
    }

    fn piece_size(&self) -> [u32; 2] {
        [self.rect[2] - self.rect[0], self.rect[3] - self.rect[1]]
    }

    /// The layer without the piece, or with it when the original stays.
    fn base(&self, keep_original: bool) -> &[u8] {
        if keep_original {
            &self.before
        } else {
            &self.rest
        }
    }
}

/// What a transform's preview shows: the box, the lattice's points and
/// whether the original stays under it.
pub(crate) type Shown = (Quad, Option<Vec<[f32; 2]>>, bool);

/// A transform in progress.
#[derive(Clone, Debug)]
pub(crate) struct TransformSession {
    /// Every layer it changes; the first is the current one.
    pub parts: Vec<Part>,
    /// The box as it began, in picture pixels: around every piece.
    pub rect: [f32; 4],
    pub mode: Mode,
    /// Every corner its own (a mode that frees them, or Ctrl on one).
    pub free: bool,
    /// Scale/Rotate: the middle, the scale each way and the turn.
    pub center: [f32; 2],
    pub scale: [f32; 2],
    pub angle: f32,
    /// The corners when they are free, in picture pixels.
    pub corners: Quad,
    /// The rotation centre and where it is when dragged.
    pub pivot: Pivot,
    pub pivot_at: [f32; 2],
    /// Mesh Transformation's lattice, in picture pixels.
    pub mesh: Option<Lattice>,
    /// What the preview last showed.
    pub shown: Option<Shown>,
    /// The selection was transformed, so it moves along.
    pub selected: bool,
    /// The layers with only their piece, for the lattice to bend.
    alone: Vec<Vec<u8>>,
}

impl TransformSession {
    fn box_size(&self) -> [f32; 2] {
        [self.rect[2] - self.rect[0], self.rect[3] - self.rect[1]]
    }

    /// The box as it stands, in picture pixels.
    pub(crate) fn quad(&self) -> Quad {
        if self.free {
            self.corners
        } else {
            let [w, h] = self.box_size();
            transform::rotated(self.center, w, h, self.scale, self.angle)
        }
    }

    /// The rotation centre now, in picture pixels.
    pub(crate) fn pivot_point(&self) -> [f32; 2] {
        self.pivot.on(&self.quad()).unwrap_or(self.pivot_at)
    }

    /// Mirrors the box: left to right when `horizontal`, else top to
    /// bottom, across its own middle.
    pub(crate) fn flip(&mut self, horizontal: bool) {
        if self.free {
            let middle = transform::middle(&self.corners);
            let axis = usize::from(!horizontal);
            for corner in &mut self.corners {
                corner[axis] = 2.0 * middle[axis] - corner[axis];
            }
        } else if horizontal {
            self.scale[0] = -self.scale[0];
        } else {
            self.scale[1] = -self.scale[1];
        }
    }

    /// Turns the box a quarter around the rotation centre, clockwise
    /// when `clockwise`.
    pub(crate) fn quarter_turn(&mut self, clockwise: bool) {
        let quarter = if clockwise {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        let pivot = self.pivot_point();
        self.turn_about(pivot, quarter, self.center, self.angle, self.quad());
    }

    /// Turned by `by` around `pivot` from how it was (`center`, `angle`,
    /// `quad`).
    fn turn_about(&mut self, pivot: [f32; 2], by: f32, center: [f32; 2], angle: f32, quad: Quad) {
        if self.free {
            self.corners = transform::turned(&quad, pivot, by);
        } else {
            let (sin, cos) = by.sin_cos();
            let (dx, dy) = (center[0] - pivot[0], center[1] - pivot[1]);
            self.center = [
                pivot[0] + dx * cos - dy * sin,
                pivot[1] + dx * sin + dy * cos,
            ];
            self.angle = angle + by;
        }
    }

    /// Back to where it started.
    fn reset(&mut self) {
        let [x0, y0, x1, y1] = self.rect;
        self.center = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        self.scale = [1.0, 1.0];
        self.angle = 0.0;
        self.corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        self.free = self.mode.frees_corners();
        if let Some(mesh) = &mut self.mesh {
            mesh.to.clone_from(&mesh.from);
        }
    }

    /// The lattice's points where they are now, if it is a mesh.
    fn points(&self) -> Option<Vec<[f32; 2]>> {
        self.mesh.as_ref().map(|mesh| mesh.to.clone())
    }

    /// The pixels of part `index` as the box puts them.
    fn render(&self, index: usize, keep_original: bool) -> Vec<u8> {
        let part = &self.parts[index];
        let [w, h] = part.size;
        if let Some(mesh) = &self.mesh {
            let [ox, oy] = part.offset.map(|value| value as f32);
            let shift = |points: &[[f32; 2]]| -> Vec<[f32; 2]> {
                points.iter().map(|[x, y]| [x - ox, y - oy]).collect()
            };
            let local = Lattice {
                columns: mesh.columns,
                rows: mesh.rows,
                from: shift(&mesh.from),
                to: shift(&mesh.to),
            };
            let bent = local.apply(&self.alone[index], w, h);
            return over(part.base(keep_original), &bent);
        }
        let quad = self.quad();
        let [bx0, by0, _, _] = self.rect;
        let [bw, bh] = self.box_size().map(|value| value.max(1.0));
        let [px0, py0, px1, py1] = part.picture_rect();
        let uv = |x: f32, y: f32| [(x - bx0) / bw, (y - by0) / bh];
        let [ox, oy] = part.offset.map(|value| value as f32);
        let local: Quad = [uv(px0, py0), uv(px1, py0), uv(px1, py1), uv(px0, py1)].map(|at| {
            let [x, y] = transform::on_quad(&quad, at);
            [x - ox, y - oy]
        });
        transform::place(
            part.base(keep_original),
            w,
            h,
            &part.piece,
            part.piece_size(),
            &local,
        )
    }
}

/// `top` (straight RGBA8) laid over `under`, the same size.
fn over(under: &[u8], top: &[u8]) -> Vec<u8> {
    let mut out = under.to_vec();
    for (pixel, source) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(top.as_chunks::<4>().0)
    {
        let a = f32::from(source[3]) / 255.0;
        if a <= 0.0 {
            continue;
        }
        let below = f32::from(pixel[3]) / 255.0;
        let alpha = a + below * (1.0 - a);
        for channel in 0..3 {
            let mixed =
                f32::from(source[channel]) * a + f32::from(pixel[channel]) * below * (1.0 - a);
            pixel[channel] = (mixed / alpha).round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }
    out
}

/// `v` turned by `angle` radians.
fn turn([x, y]: [f32; 2], angle: f32) -> [f32; 2] {
    let (sin, cos) = angle.sin_cos();
    [x * cos - y * sin, x * sin + y * cos]
}

/// The mode a transform command starts in, by its name.
#[must_use]
pub(crate) fn mode_of(command: &str) -> Mode {
    match command {
        "edit.transform.scale_only" => Mode::Scale,
        "edit.transform.rotate" => Mode::Rotate,
        "edit.transform.free" => Mode::Free,
        "edit.transform.distort" => Mode::Distort,
        "edit.transform.skew" => Mode::Skew,
        "edit.transform.perspective" => Mode::Perspective,
        _ => Mode::ScaleRotate,
    }
}

impl SuePaint {
    /// The layers a transform changes: the current layer, or the raster
    /// layers of the current folder, and those of the layers picked with
    /// it (Ctrl+click in the Layer palette). A layer that cannot be drawn
    /// on (locked, hidden, vector, text) is left as it is.
    fn transform_targets(&self, cx: &AppletContext<'_>) -> Result<Vec<doc::LayerInfo>, String> {
        let arena = &cx.world.arena;
        let Some(picture) = self.picture() else {
            return Err(tr!("pick-layer-draw-first"));
        };
        let current = self.layer_info(arena).ok_or(tr!("pick-layer-draw-first"))?;
        let mut ids = vec![current.id];
        if let Some(doc) = self.doc() {
            for id in &doc.picked {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
        }
        let mut wanted = Vec::new();
        for id in ids {
            if doc::is_folder(arena, id) {
                wanted.extend(doc::members(arena, picture, id));
            } else {
                wanted.push(id);
            }
        }
        if wanted.len() == 1 && wanted[0] == current.id {
            // One layer: say why it cannot be, as a stroke would.
            return self.paintable(arena).map(|info| vec![info]);
        }
        let layers = doc::layers(arena, picture);
        let found: Vec<doc::LayerInfo> = layers
            .into_iter()
            .filter(|info| wanted.contains(&info.id))
            .filter(|info| {
                info.kind == sue_model::picture::LayerKind::Raster
                    && !info.locked
                    && info.style.visible
                    && !crate::vector::is_vector(arena, info.id)
                    && doc::raster(arena, info.id)
                        .is_some_and(|raster| raster.format() == sue_core::PixelFormat::Rgba8)
            })
            .collect();
        if found.is_empty() {
            return Err(tr!("nothing-to-transform-here"));
        }
        // The current layer first, the rest in stacking order.
        let mut ordered: Vec<doc::LayerInfo> = Vec::with_capacity(found.len());
        let (first, others): (Vec<_>, Vec<_>) =
            found.into_iter().partition(|info| info.id == current.id);
        ordered.extend(first);
        ordered.extend(others);
        Ok(ordered)
    }

    /// Starts a transform in `mode`, or Mesh Transformation with `mesh`.
    pub(crate) fn start_transform_in(
        &mut self,
        cx: &mut AppletContext<'_>,
        mode: Mode,
        mesh: bool,
    ) {
        if self.transform.is_some() || self.warp.is_some() {
            return;
        }
        let targets = match self.transform_targets(cx) {
            Ok(targets) => targets,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let mut parts = Vec::new();
        for info in targets {
            let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
                continue;
            };
            let Some(before) = raster.bytes().map(<[u8]>::to_vec) else {
                continue;
            };
            let size = [raster.width(), raster.height()];
            let shifted = selection
                .as_ref()
                .map(|selection| selection.shifted(info.style.offset));
            let Some((rect, piece, rest)) =
                transform::lift(&before, size[0], size[1], shifted.as_ref())
            else {
                continue;
            };
            parts.push(Part {
                layer: info.id,
                size,
                before,
                rest,
                piece,
                rect,
                offset: info.style.offset,
            });
        }
        if parts.is_empty() {
            cx.notify(tr!("nothing-to-transform-here"));
            return;
        }
        let mut rect = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for part in &parts {
            let [x0, y0, x1, y1] = part.picture_rect();
            rect = [
                rect[0].min(x0),
                rect[1].min(y0),
                rect[2].max(x1),
                rect[3].max(y1),
            ];
        }
        let alone = if mesh {
            parts
                .iter()
                .map(|part| {
                    let mut only = vec![0; part.before.len()];
                    let [w, _] = part.size;
                    let [x0, y0, x1, y1] = part.rect;
                    let pw = (x1 - x0) as usize;
                    for y in y0..y1 {
                        let to = ((y * w + x0) * 4) as usize;
                        let from = ((y - y0) as usize * pw) * 4;
                        only[to..to + pw * 4].copy_from_slice(&part.piece[from..from + pw * 4]);
                    }
                    only
                })
                .collect()
        } else {
            Vec::new()
        };
        let [columns, rows] = self.transform_prefs.mesh;
        let mut session = TransformSession {
            parts,
            rect,
            mode,
            free: mode.frees_corners(),
            center: [0.0; 2],
            scale: [1.0, 1.0],
            angle: 0.0,
            corners: [[0.0; 2]; 4],
            pivot: self.transform_prefs.pivot,
            pivot_at: [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0],
            mesh: mesh.then(|| Lattice::over(rect, columns, rows)),
            shown: None,
            selected: selection.is_some(),
            alone,
        };
        if session.pivot == Pivot::Free {
            session.pivot = Pivot::Middle;
        }
        session.reset();
        session.shown = Some((session.quad(), session.points(), false));
        self.transform = Some(session);
    }

    /// Shows the layers with the pieces where the box puts them.
    pub(crate) fn transform_preview(&mut self, cx: &mut AppletContext<'_>) {
        let keep = self.transform_prefs.keep_original;
        let Some(session) = self.transform.as_mut() else {
            return;
        };
        let now = (session.quad(), session.points(), keep);
        if session.shown.as_ref() == Some(&now) {
            return;
        }
        for index in 0..session.parts.len() {
            let pixels = session.render(index, keep);
            if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, session.parts[index].layer)
                .and_then(RasterTile2D::bytes_mut)
                .filter(|bytes| bytes.len() == pixels.len())
            {
                bytes.copy_from_slice(&pixels);
            }
        }
        session.shown = Some(now);
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Ends the transform: keeps it as one undo step for every layer it
    /// changed (the selection moves with it), or puts the layers back.
    pub(crate) fn finish_transform(&mut self, cx: &mut AppletContext<'_>, keep: bool) {
        let Some(session) = self.transform.take() else {
            return;
        };
        self.gesture = None;
        let keep_original = self.transform_prefs.keep_original;
        let label = if session.mesh.is_some() {
            tr!("mesh-transformation")
        } else if session.free {
            tr!("free-transform")
        } else {
            tr!("scale-rotate")
        };
        let quad = session.quad();
        if !keep {
            for part in &session.parts {
                if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, part.layer)
                    .and_then(RasterTile2D::bytes_mut)
                    .filter(|bytes| bytes.len() == part.before.len())
                {
                    bytes.copy_from_slice(&part.before);
                }
            }
            if let Some(doc) = self.docs.get_mut(self.active) {
                doc.compositor
                    .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
            }
            return;
        }
        crate::launchers::one_step(cx, label.clone(), |cx| {
            for index in 0..session.parts.len() {
                // Past the layer's edges (the canvas's, for most layers):
                // the layer grows to keep all of it, as Clip Studio Paint
                // keeps what is moved off the canvas, so moving it back
                // brings it back.
                if session.mesh.is_none()
                    && Self::grow_for_transform(cx, &session, index, &quad, keep_original, &label)
                {
                    continue;
                }
                let part = &session.parts[index];
                let after = session.render(index, keep_original);
                if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, part.layer)
                    .and_then(RasterTile2D::bytes_mut)
                    .filter(|bytes| bytes.len() == after.len())
                {
                    bytes.copy_from_slice(&after);
                }
                if let Some(patch) =
                    doc::diff_patch(&part.before, &after, part.size[0], part.size[1])
                {
                    cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                        part.layer,
                        label.clone(),
                        vec![patch],
                    )));
                }
            }
        });
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
        if session.selected
            && session.mesh.is_none()
            && let Some(doc) = self.doc_mut()
            && let Some(old) = doc.selection.as_ref()
        {
            let [width, height] = old.size();
            let mut moved = crate::select::Selection::new(width, height);
            moved.polygon(&quad, crate::select::Op::Replace);
            doc.set_selection(Some(moved));
        }
    }

    /// Keeps the transform of part `index` when it reaches past the
    /// layer's edges: the layer grows to hold all of it and moves by as
    /// much. `false` (nothing done) when the result fits the layer.
    fn grow_for_transform(
        cx: &mut AppletContext<'_>,
        session: &TransformSession,
        index: usize,
        quad: &Quad,
        keep_original: bool,
        label: &str,
    ) -> bool {
        let part = &session.parts[index];
        let [width, height] = part.size;
        let [ox, oy] = part.offset.map(|value| value as f32);
        // Where this part's corners land, in the layer's pixels.
        let [bx0, by0, _, _] = session.rect;
        let [bw, bh] = session.box_size().map(|value| value.max(1.0));
        let [px0, py0, px1, py1] = part.picture_rect();
        let uv = |x: f32, y: f32| [(x - bx0) / bw, (y - by0) / bh];
        let local: Quad = [uv(px0, py0), uv(px1, py0), uv(px1, py1), uv(px0, py1)].map(|at| {
            let [x, y] = transform::on_quad(quad, at);
            [x - ox, y - oy]
        });
        let (mut low, mut high) = ([0.0_f32, 0.0], [width as f32, height as f32]);
        for [x, y] in local {
            low = [low[0].min(x), low[1].min(y)];
            high = [high[0].max(x), high[1].max(y)];
        }
        let low = low.map(|value| value.floor() as i64);
        let high = high.map(|value| value.ceil() as i64);
        if low == [0, 0] && high == [i64::from(width), i64::from(height)] {
            return false;
        }
        let (grown_width, grown_height) = ((high[0] - low[0]) as u32, (high[1] - low[1]) as u32);
        if grown_width > 16_384 || grown_height > 16_384 {
            return false;
        }
        // What stays of the layer, placed in the grown one.
        let mut base = vec![0_u8; grown_width as usize * grown_height as usize * 4];
        let (dx, dy) = ((-low[0]) as usize, (-low[1]) as usize);
        let row = width as usize * 4;
        let source = part.base(keep_original);
        for y in 0..height as usize {
            let at = ((y + dy) * grown_width as usize + dx) * 4;
            base[at..at + row].copy_from_slice(&source[y * row..(y + 1) * row]);
        }
        let moved: Quad = local.map(|[x, y]| [x - low[0] as f32, y - low[1] as f32]);
        let after = transform::place(
            &base,
            grown_width,
            grown_height,
            &part.piece,
            part.piece_size(),
            &moved,
        );
        let Ok(grown) = RasterTile2D::from_bytes(
            grown_width,
            grown_height,
            sue_core::PixelFormat::Rgba8,
            after,
        ) else {
            return false;
        };
        // The layer as it was, so the step's undo has it.
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, part.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == part.before.len())
        {
            bytes.copy_from_slice(&part.before);
        }
        let offset = [part.offset[0] + low[0], part.offset[1] + low[1]];
        let edit = sue_core::txn::Batch::new(label.to_owned())
            .then(sue_core::txn::ReplaceComponent::new(
                part.layer, grown, label,
            ))
            .then(sue_core::txn::SetProperty::new(
                part.layer,
                sue_model::picture::X,
                offset[0],
            ))
            .then(sue_core::txn::SetProperty::new(
                part.layer,
                sue_model::picture::Y,
                offset[1],
            ));
        Self::apply(cx, edit)
    }

    /// A press while transforming: grabs the rotation centre, a lattice
    /// point, a handle, the inside or the outside.
    pub(crate) fn transform_press(
        &mut self,
        pos: Pos2,
        view: View,
        viewport: Rect,
        size: [u32; 2],
    ) {
        let Some(session) = self.transform.as_ref() else {
            return;
        };
        let reach = crate::canvas::marks::GRAB_REACH;
        let quad = session.quad();
        let screen = |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0], p[1]));
        let near = |p: [f32; 2]| screen(p).distance(pos) <= reach;
        let p = view.to_picture(viewport, size, pos);
        let at = [p.x, p.y];
        let pivot = session.pivot_point();
        let handle = if let Some(mesh) = &session.mesh {
            mesh.to
                .iter()
                .position(|point| near(*point))
                .map_or(Handle::Move, Handle::Point)
        } else if session.mode != Mode::Rotate && (0..4).any(|corner| near(quad[corner])) {
            Handle::Corner((0..4).find(|&corner| near(quad[corner])).unwrap_or(0))
        } else if screen(pivot).distance(pos) <= reach / 2.0 && !session.mode.frees_corners() {
            Handle::Pivot
        } else if let Some(edge) = (0..4).find(|&edge| {
            let (a, b) = (quad[edge], quad[(edge + 1) % 4]);
            session.mode != Mode::Rotate && near([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
        }) {
            Handle::Edge(edge)
        } else if (crate::manga::signed_distance(&quad, at) <= 0.0 && session.mode != Mode::Rotate)
            || (session.mode.frees_corners() && session.mode != Mode::Free)
        {
            Handle::Move
        } else {
            Handle::Rotate
        };
        let grab = match handle {
            Handle::Pivot => pivot,
            Handle::Point(index) => session.mesh.as_ref().map_or(at, |mesh| mesh.to[index]),
            _ => pivot,
        };
        self.gesture = Some(Gesture::Transform {
            handle,
            start: at,
            quad,
            center: session.center,
            scale: session.scale,
            angle: session.angle,
            grab,
            points: session.points().unwrap_or_default(),
        });
    }

    /// A drag while transforming, to `p` in picture pixels.
    pub(crate) fn transform_drag(&mut self, p: [f32; 2], modifiers: Modifiers) {
        let Some(Gesture::Transform {
            handle,
            start,
            quad,
            center,
            scale,
            angle,
            grab,
            ref points,
        }) = self.gesture
        else {
            return;
        };
        let points = points.clone();
        let keep_aspect = self.transform_prefs.keep_aspect;
        let Some(session) = self.transform.as_mut() else {
            return;
        };
        let [w, h] = session.box_size();
        let delta = [p[0] - start[0], p[1] - start[1]];
        let step = if modifiers.shift {
            15_f32.to_radians()
        } else {
            0.0
        };
        match handle {
            Handle::Pivot => {
                session.pivot = Pivot::Free;
                session.pivot_at = [grab[0] + delta[0], grab[1] + delta[1]];
                return;
            }
            Handle::Point(index) => {
                if let Some(mesh) = session.mesh.as_mut()
                    && let Some(point) = mesh.to.get_mut(index)
                {
                    *point = [grab[0] + delta[0], grab[1] + delta[1]];
                }
                return;
            }
            Handle::Move if session.mesh.is_some() => {
                if let Some(mesh) = session.mesh.as_mut() {
                    for (point, was) in mesh.to.iter_mut().zip(&points) {
                        *point = [was[0] + delta[0], was[1] + delta[1]];
                    }
                }
                return;
            }
            Handle::Rotate => {
                let by = transform::drag_angle(grab, start, p, step);
                session.turn_about(grab, by, center, angle, quad);
                return;
            }
            _ => {}
        }
        // Ctrl on a corner distorts, whatever the mode.
        if let Handle::Corner(i) = handle
            && modifiers.command
            && !session.free
        {
            session.free = true;
            session.corners = quad;
            session.corners[i] = p;
            return;
        }
        if session.free {
            let mut corners = quad;
            match handle {
                Handle::Corner(i) if modifiers.command => corners[i] = p,
                Handle::Corner(i) => {
                    let pivot = transform::middle(&quad);
                    corners =
                        transform::drag_corner(&quad, session.mode, i, p, pivot, modifiers.shift);
                }
                Handle::Edge(i) => {
                    for corner in [i, (i + 1) % 4] {
                        corners[corner] = [quad[corner][0] + delta[0], quad[corner][1] + delta[1]];
                    }
                }
                _ => corners = quad.map(|[x, y]| [x + delta[0], y + delta[1]]),
            }
            session.corners = corners;
            if session.pivot == Pivot::Free {
                // A dragged centre stays where it was put.
            }
            return;
        }
        match handle {
            Handle::Move => {
                session.center = [center[0] + delta[0], center[1] + delta[1]];
                if session.pivot == Pivot::Free {
                    session.pivot_at = [grab[0] + delta[0], grab[1] + delta[1]];
                }
            }
            Handle::Corner(i) if session.mode == Mode::Rotate => {
                let by = transform::drag_angle(grab, quad[i], p, step);
                session.turn_about(grab, by, center, angle, quad);
            }
            Handle::Corner(i) => {
                let fixed = quad[(i + 2) % 4];
                let local = turn([p[0] - fixed[0], p[1] - fixed[1]], -angle);
                let (mut sx, mut sy) = (local[0].abs() / w.max(1.0), local[1].abs() / h.max(1.0));
                // Keep aspect ratio, unless Shift says otherwise (or
                // Shift keeps it when the setting is off).
                if keep_aspect != modifiers.shift {
                    let both = sx.max(sy);
                    (sx, sy) = (both, both);
                }
                let half = [
                    local[0].signum() * w * sx / 2.0,
                    local[1].signum() * h * sy / 2.0,
                ];
                let shift = turn(half, angle);
                session.scale = [sx.max(0.01), sy.max(0.01)];
                session.center = [fixed[0] + shift[0], fixed[1] + shift[1]];
            }
            Handle::Edge(i) => {
                let (a, b) = (quad[(i + 2) % 4], quad[(i + 3) % 4]);
                let fixed = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let local = turn([p[0] - fixed[0], p[1] - fixed[1]], -angle);
                let mut next = scale;
                let half = if i % 2 == 0 {
                    next[1] = (local[1].abs() / h.max(1.0)).max(0.01);
                    [0.0, local[1].signum() * h * next[1] / 2.0]
                } else {
                    next[0] = (local[0].abs() / w.max(1.0)).max(0.01);
                    [local[0].signum() * w * next[0] / 2.0, 0.0]
                };
                let shift = turn(half, angle);
                session.scale = next;
                session.center = [fixed[0] + shift[0], fixed[1] + shift[1]];
            }
            Handle::Rotate | Handle::Pivot | Handle::Point(_) => {}
        }
    }

    /// The box, its handles, the rotation centre or the lattice, and the
    /// bar (the Transform's settings, Reset, Cancel, OK; Enter keeps, Esc
    /// puts the layers back), and the Transform launcher under the box.
    pub(crate) fn transform_ui(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let Some(session) = self.transform.as_ref() else {
            return;
        };
        let screen = |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0], p[1]));
        let quad = session.quad();
        let corners: Vec<Pos2> = quad.iter().map(|p| screen(*p)).collect();
        let painter = ui.painter().with_clip_rect(viewport);
        let held = match &self.gesture {
            Some(Gesture::Transform { handle, .. }) => Some(*handle),
            _ => None,
        };
        if let Some(mesh) = &session.mesh {
            let at = |column: usize, row: usize| screen(mesh.to[row * mesh.columns + column]);
            for row in 0..mesh.rows {
                let line: Vec<Pos2> = (0..mesh.columns).map(|column| at(column, row)).collect();
                crate::canvas::marks::guide(&painter, line, false, theme);
            }
            for column in 0..mesh.columns {
                let line: Vec<Pos2> = (0..mesh.rows).map(|row| at(column, row)).collect();
                crate::canvas::marks::guide(&painter, line, false, theme);
            }
            for (index, point) in mesh.to.iter().enumerate() {
                crate::canvas::marks::dot(
                    &painter,
                    screen(*point),
                    held == Some(Handle::Point(index)),
                    theme,
                );
            }
        } else {
            crate::canvas::marks::guide(&painter, corners.clone(), true, theme);
            let rotate_only = session.mode == Mode::Rotate;
            for (index, at) in corners.iter().enumerate() {
                if !rotate_only {
                    crate::canvas::marks::handle(
                        &painter,
                        *at,
                        held == Some(Handle::Corner(index)),
                        theme,
                    );
                }
            }
            if !rotate_only {
                for edge in 0..4 {
                    let (a, b) = (corners[edge], corners[(edge + 1) % 4]);
                    crate::canvas::marks::handle(
                        &painter,
                        pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                        held == Some(Handle::Edge(edge)),
                        theme,
                    );
                }
            }
            if !session.mode.frees_corners() {
                crate::canvas::marks::pivot(
                    &painter,
                    screen(session.pivot_point()),
                    held == Some(Handle::Pivot),
                    theme,
                );
            }
        }
        for (corner, at) in corners.iter().enumerate() {
            crate::launchers::note_mark(ui.ctx(), &format!("transform.corner.{corner}"), *at);
        }
        if let Some(session) = self.transform.as_ref() {
            crate::launchers::note_mark(ui.ctx(), "transform.pivot", screen(session.pivot_point()));
        }
        let action = self.transform_bar(ui, viewport, cx, theme);
        // The Transform launcher, under the box.
        let mut action = action;
        if self.transform_launcher && self.gesture.is_none() {
            let slots = [
                Button::own("transform.reset", Icon::Reset, tr!("reset")),
                Button::own(
                    "transform.flip_h",
                    Icon::FlipHorizontal,
                    tr!("flip-horizontal"),
                ),
                Button::own("transform.flip_v", Icon::FlipVertical, tr!("flip-vertical")),
                Button::own(
                    "transform.left",
                    Icon::RotateLeft,
                    tr!("rotate-90-degrees-left"),
                ),
                Button::own(
                    "transform.right",
                    Icon::RotateRight,
                    tr!("rotate-90-degrees-right"),
                ),
                Button::own("transform.cancel", Icon::Cancel, tr!("cancel")),
                Button::own("transform.ok", Icon::Confirm, tr!("ok")),
            ]
            .map(Slot::Button);
            // A lattice is not flipped or turned whole: Reset, Cancel, OK.
            let slots: Vec<Slot> = if self
                .transform
                .as_ref()
                .is_some_and(|session| session.mesh.is_some())
            {
                [&slots[0], &slots[5], &slots[6]]
                    .into_iter()
                    .cloned()
                    .collect()
            } else {
                slots.to_vec()
            };
            let (clicked, _) = crate::launchers::bar(ui, viewport, &corners, &slots, theme);
            if let Some(session) = self.transform.as_mut() {
                match clicked.as_deref() {
                    Some("launcher.transform.reset") => action = Some(BarAction::Reset),
                    Some("launcher.transform.flip_h") => session.flip(true),
                    Some("launcher.transform.flip_v") => session.flip(false),
                    Some("launcher.transform.left") => session.quarter_turn(false),
                    Some("launcher.transform.right") => session.quarter_turn(true),
                    Some("launcher.transform.cancel") => action = Some(BarAction::Cancel),
                    Some(_) => action = Some(BarAction::Ok),
                    None => {}
                }
            }
        }
        let (enter, escape) = ui.input(|input| {
            (
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
            )
        });
        let typing = ui.ctx().text_edit_focused();
        if enter && !typing {
            action = Some(BarAction::Ok);
        } else if escape && !typing {
            action = Some(BarAction::Cancel);
        }
        match action {
            Some(BarAction::Reset) => {
                if let Some(session) = self.transform.as_mut() {
                    session.reset();
                }
                self.transform_preview(cx);
            }
            Some(BarAction::Cancel) => self.finish_transform(cx, false),
            Some(BarAction::Ok) => self.finish_transform(cx, true),
            None => self.transform_preview(cx),
        }
    }

    /// The bar over the canvas while transforming: the Transform's Tool
    /// Property (mode, rotation centre, Keep aspect ratio, Keep original
    /// image, scale, angle and position; for Mesh Transformation its
    /// points across and down) and Reset, Cancel and OK.
    #[allow(clippy::too_many_lines, reason = "one control after another")]
    fn transform_bar(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> Option<BarAction> {
        let Self {
            transform,
            transform_prefs: prefs,
            ..
        } = self;
        let session = transform.as_mut()?;
        let mut action = None;
        let gap = theme.space(Space::S2);
        // Wide enough for "-2000.0 %", no wider.
        let field_width = theme.space(Space::S9) + theme.space(Space::S7);
        let frame = egui::Frame::new()
            .fill(theme.surface(2))
            .stroke(theme.hairline())
            .corner_radius(theme.corner(Radius::Lg))
            .shadow(theme.elevation(Elevation::Raised))
            .inner_margin(egui::Margin::same(theme.space(Space::S2).round() as i8));
        let shown = egui::Area::new(Id::unique("sue-paint-transform-bar"))
            .fade_in(false)
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(viewport.center_top() + Vec2::Y * gap)
            .constrain_to(viewport)
            .show(ui.ctx(), |ui| {
                frame.show(ui, |ui| {
                    ui.set_max_width(viewport.width() - theme.space(Space::S8));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(gap);
                        if let Some(mesh) = &session.mesh {
                            ui.label(
                                egui::RichText::new(tr!("mesh-transformation"))
                                    .font(theme.text_font(sue_wm::ui::TextRole::Body))
                                    .color(theme.text),
                            );
                            let (mut columns, mut rows) = (mesh.columns as f64, mesh.rows as f64);
                            let across = tr!("mesh-points-across");
                            let down = tr!("mesh-points-down");
                            caption(ui, &tr!("mesh-across"), theme);
                            let a = sue_wm::ui::number_field("sue-paint-mesh-across", &mut columns)
                                .range(2.0..=Lattice::MOST as f64)
                                .width(field_width)
                                .label(&across)
                                .show(ui, theme);
                            sue_wm::ui::tooltip(a.clone(), &Tip::new(&across), theme);
                            caption(ui, &tr!("mesh-down"), theme);
                            let d = sue_wm::ui::number_field("sue-paint-mesh-down", &mut rows)
                                .range(2.0..=Lattice::MOST as f64)
                                .width(field_width)
                                .label(&down)
                                .show(ui, theme);
                            sue_wm::ui::tooltip(d.clone(), &Tip::new(&down), theme);
                            let wanted = [columns as usize, rows as usize];
                            if wanted != [mesh.columns, mesh.rows] {
                                prefs.mesh = wanted;
                                session.mesh =
                                    Some(Lattice::over(session.rect, wanted[0], wanted[1]));
                            }
                        } else {
                            let names: Vec<String> =
                                Mode::ALL.iter().map(|mode| mode.label()).collect();
                            let mut chosen = Mode::ALL
                                .iter()
                                .position(|mode| *mode == session.mode)
                                .unwrap_or(0);
                            let mode_name = tr!("transform-mode");
                            let picked = sue_wm::ui::dropdown(
                                "sue-paint-transform-mode",
                                &mut chosen,
                                &names,
                            )
                            .label(&mode_name)
                            .show(ui, theme);
                            if picked.changed() {
                                let quad = session.quad();
                                session.mode = Mode::ALL[chosen];
                                if session.mode.frees_corners() && !session.free {
                                    session.free = true;
                                    session.corners = quad;
                                }
                            }
                            sue_wm::ui::tooltip(picked, &Tip::new(&mode_name), theme);
                            if !session.mode.frees_corners() {
                                let pivots: Vec<String> =
                                    Pivot::LIST.iter().map(|pivot| pivot.label()).collect();
                                let mut at = Pivot::LIST
                                    .iter()
                                    .position(|pivot| {
                                        *pivot == session.pivot
                                            || (session.pivot == Pivot::Point(1, 1)
                                                && *pivot == Pivot::Middle)
                                    })
                                    .unwrap_or(4);
                                let centre = tr!("center-of-rotation");
                                caption(ui, &centre, theme);
                                let moved = sue_wm::ui::dropdown(
                                    "sue-paint-transform-pivot",
                                    &mut at,
                                    &pivots,
                                )
                                .label(&centre)
                                .show(ui, theme);
                                if moved.changed() {
                                    let point = session.pivot_point();
                                    session.pivot = Pivot::LIST[at];
                                    if session.pivot == Pivot::Free {
                                        session.pivot_at = point;
                                    } else {
                                        prefs.pivot = session.pivot;
                                    }
                                }
                                sue_wm::ui::tooltip(
                                    moved,
                                    &Tip::new(&centre).help(&tr!("center-of-rotation-help")),
                                    theme,
                                );
                            }
                            if !session.free {
                                let mut width = f64::from(session.scale[0] * 100.0);
                                let mut height = f64::from(session.scale[1] * 100.0);
                                let mut degrees = f64::from(session.angle.to_degrees());
                                let mut x = f64::from(session.center[0]);
                                let mut y = f64::from(session.center[1]);
                                let names = [
                                    tr!("width-2"),
                                    tr!("height-2"),
                                    tr!("angle"),
                                    tr!("position-x"),
                                    tr!("position-y"),
                                ];
                                let degree = tr!("deg");
                                let mut typed = false;
                                for (index, (value, (suffix, range))) in
                                    [&mut width, &mut height, &mut degrees, &mut x, &mut y]
                                        .into_iter()
                                        .zip([
                                            (" %", -2000.0..=2000.0),
                                            (" %", -2000.0..=2000.0),
                                            (degree.as_str(), -360.0..=360.0),
                                            (" px", -100_000.0..=100_000.0),
                                            (" px", -100_000.0..=100_000.0),
                                        ])
                                        .enumerate()
                                {
                                    caption(ui, &names[index], theme);
                                    let field = sue_wm::ui::number_field(
                                        ("sue-paint-transform-number", index),
                                        value,
                                    )
                                    .range(range)
                                    .width(field_width)
                                    .decimals(usize::from(index == 2))
                                    .suffix(suffix)
                                    .label(&names[index])
                                    .show(ui, theme);
                                    typed |= field.changed();
                                    sue_wm::ui::tooltip(field, &Tip::new(&names[index]), theme);
                                }
                                if typed {
                                    session.scale =
                                        [(width / 100.0) as f32, (height / 100.0) as f32];
                                    session.angle = (degrees as f32).to_radians();
                                    session.center = [x as f32, y as f32];
                                }
                                let aspect = tr!("keep-aspect-ratio");
                                let flip = sue_wm::ui::switch(
                                    "sue-paint-transform-aspect",
                                    &aspect,
                                    prefs.keep_aspect,
                                )
                                .show(ui, theme);
                                if flip.clicked() {
                                    prefs.keep_aspect = !prefs.keep_aspect;
                                }
                                sue_wm::ui::tooltip(
                                    flip,
                                    &Tip::new(&aspect).shortcut("Shift"),
                                    theme,
                                );
                            }
                        }
                        let original = tr!("keep-original-image");
                        let flip = sue_wm::ui::switch(
                            "sue-paint-transform-original",
                            &original,
                            prefs.keep_original,
                        )
                        .show(ui, theme);
                        if flip.clicked() {
                            prefs.keep_original = !prefs.keep_original;
                        }
                        sue_wm::ui::tooltip(flip, &Tip::new(&original), theme);
                        let reset = tr!("reset");
                        let pressed = sue_wm::ui::button("sue-paint-transform-reset", &reset)
                            .icon(Icon::Reset)
                            .show(ui, theme);
                        if pressed.clicked() {
                            action = Some(BarAction::Reset);
                        }
                        sue_wm::ui::tooltip(pressed, &Tip::new(&reset), theme);
                        let cancel = tr!("cancel");
                        let pressed = sue_wm::ui::button("sue-paint-transform-cancel", &cancel)
                            .show(ui, theme);
                        if pressed.clicked() {
                            action = Some(BarAction::Cancel);
                        }
                        sue_wm::ui::tooltip(pressed, &Tip::new(&cancel).shortcut("Esc"), theme);
                        let ok = tr!("ok");
                        let pressed = sue_wm::ui::button("sue-paint-transform-ok", &ok)
                            .primary()
                            .show(ui, theme);
                        if pressed.clicked() {
                            action = Some(BarAction::Ok);
                        }
                        sue_wm::ui::tooltip(pressed, &Tip::new(&ok).shortcut("Enter"), theme);
                    });
                });
            });
        crate::launchers::note(ui.ctx(), shown.response.rect);
        let _ = cx;
        action
    }
}

/// A small muted name before the control it names, on a bar.
pub(crate) fn caption(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(sue_wm::ui::TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// What a press on a frame with handles grabbed (the camera's, the light
/// table's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameGrip {
    /// Inside, or anywhere that is not a handle: moves it.
    Inside,
    /// A corner, 0 top left, then clockwise: scales it.
    Corner(usize),
    /// The middle of an edge, 0 top, then clockwise: scales it.
    Edge(usize),
    /// The handle on its stalk, or outside it: turns it.
    Turn,
}

/// Where the turning handle of `quad` (screen points) is: from the middle
/// of its top edge, `length` further out from its middle.
#[must_use]
pub(crate) fn stalk_of(quad: &[Pos2; 4], length: f32) -> (Pos2, Pos2) {
    let top = pos2((quad[0].x + quad[1].x) / 2.0, (quad[0].y + quad[1].y) / 2.0);
    let middle = pos2(
        quad.iter().map(|at| at.x).sum::<f32>() / 4.0,
        quad.iter().map(|at| at.y).sum::<f32>() / 4.0,
    );
    let out = (top - middle).normalized();
    let out = if out.x.is_finite() && out.length() > 0.5 {
        out
    } else {
        egui::Vec2::new(0.0, -1.0)
    };
    (top, top + out * length)
}

/// What a press at `pos` grabs of `quad` (screen points), whose turning
/// handle is at `stalk`; a press outside the frame turns it when
/// `outside_turns`, else moves it.
#[must_use]
pub(crate) fn frame_grip(
    quad: &[Pos2; 4],
    stalk: Pos2,
    pos: Pos2,
    outside_turns: bool,
) -> FrameGrip {
    let reach = crate::canvas::marks::GRAB_REACH;
    if stalk.distance(pos) <= reach {
        return FrameGrip::Turn;
    }
    if let Some(corner) = (0..4).find(|&corner| quad[corner].distance(pos) <= reach) {
        return FrameGrip::Corner(corner);
    }
    if let Some(edge) = (0..4).find(|&edge| {
        let (a, b) = (quad[edge], quad[(edge + 1) % 4]);
        pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0).distance(pos) <= reach
    }) {
        return FrameGrip::Edge(edge);
    }
    let points = quad.map(|at| [at.x, at.y]);
    if outside_turns && !crate::camera::inside(&points, [pos.x, pos.y]) {
        FrameGrip::Turn
    } else {
        FrameGrip::Inside
    }
}

/// The point `grip` holds on `quad` (picture pixels) and the point it
/// scales about: the opposite corner or edge, or the middle with Alt.
#[must_use]
pub(crate) fn grip_points(
    quad: &Quad,
    grip: FrameGrip,
    from_middle: bool,
) -> Option<([f32; 2], [f32; 2])> {
    let middle = transform::middle(quad);
    let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let (held, across) = match grip {
        FrameGrip::Corner(i) => (quad[i % 4], quad[(i + 2) % 4]),
        FrameGrip::Edge(i) => (
            mid(quad[i % 4], quad[(i + 1) % 4]),
            mid(quad[(i + 2) % 4], quad[(i + 3) % 4]),
        ),
        FrameGrip::Inside | FrameGrip::Turn => return None,
    };
    Some((held, if from_middle { middle } else { across }))
}

/// How much a frame scales when the point `held` (picture pixels) is
/// dragged to `to` while `anchor` stays: along the line between them.
#[must_use]
pub(crate) fn scale_ratio(anchor: [f32; 2], held: [f32; 2], to: [f32; 2]) -> f32 {
    let along = [held[0] - anchor[0], held[1] - anchor[1]];
    let length = along[0] * along[0] + along[1] * along[1];
    if length < 1e-6 {
        return 1.0;
    }
    (((to[0] - anchor[0]) * along[0] + (to[1] - anchor[1]) * along[1]) / length).max(0.01)
}

/// A move of `by` kept to the axis it goes along most (Shift).
#[must_use]
pub(crate) fn constrained(by: [f32; 2], shift: bool) -> [f32; 2] {
    if !shift {
        by
    } else if by[0].abs() >= by[1].abs() {
        [by[0], 0.0]
    } else {
        [0.0, by[1]]
    }
}

/// What the transform bar or launcher asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarAction {
    Reset,
    Cancel,
    Ok,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(mode: Mode) -> TransformSession {
        let mut session = TransformSession {
            parts: Vec::new(),
            rect: [10.0, 10.0, 30.0, 20.0],
            mode,
            free: false,
            center: [0.0; 2],
            scale: [1.0, 1.0],
            angle: 0.0,
            corners: [[0.0; 2]; 4],
            pivot: Pivot::Middle,
            pivot_at: [20.0, 15.0],
            mesh: None,
            shown: None,
            selected: false,
            alone: Vec::new(),
        };
        session.reset();
        session
    }

    fn grab(paint: &mut SuePaint, handle: Handle, start: [f32; 2]) {
        let session = paint.transform.as_ref().unwrap();
        paint.gesture = Some(Gesture::Transform {
            handle,
            start,
            quad: session.quad(),
            center: session.center,
            scale: session.scale,
            angle: session.angle,
            grab: match handle {
                Handle::Point(index) => session.mesh.as_ref().unwrap().to[index],
                _ => session.pivot_point(),
            },
            points: session.points().unwrap_or_default(),
        });
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn handles_scale_stretch_turn_and_bend_the_box() {
        let mut paint = SuePaint {
            transform: Some(session(Mode::ScaleRotate)),
            ..SuePaint::default()
        };
        // The bottom right corner out to twice the size: the top left stays.
        grab(&mut paint, Handle::Corner(2), [30.0, 20.0]);
        paint.transform_drag([50.0, 30.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert!(close(quad[0], [10.0, 10.0]) && close(quad[2], [50.0, 30.0]));
        // With Shift, the aspect is free.
        paint.transform = Some(session(Mode::ScaleRotate));
        grab(&mut paint, Handle::Corner(2), [30.0, 20.0]);
        paint.transform_drag([50.0, 20.0], Modifiers::SHIFT);
        assert_eq!(paint.transform.as_ref().unwrap().scale, [2.0, 1.0]);
        // With Keep aspect ratio off, a plain drag is free and Shift keeps.
        paint.transform_prefs.keep_aspect = false;
        paint.transform = Some(session(Mode::ScaleRotate));
        grab(&mut paint, Handle::Corner(2), [30.0, 20.0]);
        paint.transform_drag([50.0, 20.0], Modifiers::NONE);
        assert_eq!(paint.transform.as_ref().unwrap().scale, [2.0, 1.0]);
        paint.transform_prefs.keep_aspect = true;
        // Outside the box turns it around its middle.
        paint.transform = Some(session(Mode::ScaleRotate));
        grab(&mut paint, Handle::Rotate, [40.0, 15.0]);
        paint.transform_drag([20.0, 35.0], Modifiers::NONE);
        let angle = paint.transform.as_ref().unwrap().angle;
        assert!(
            (angle - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "{angle}"
        );
        // The launcher's flips mirror the box; a quarter turn turns it.
        let mut flipped = session(Mode::ScaleRotate);
        flipped.flip(true);
        assert!(
            (flipped.quad()[0][0] - 30.0).abs() < 1e-3,
            "the left edge went right"
        );
        flipped.flip(true);
        assert_eq!(flipped.scale, [1.0, 1.0]);
        let mut turned = session(Mode::ScaleRotate);
        turned.quarter_turn(true);
        assert!((turned.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let mut free = session(Mode::Free);
        free.flip(false);
        assert!(
            (free.corners[0][1] - 20.0).abs() < 1e-3,
            "the top went down"
        );
        // Free Transform moves one corner alone.
        paint.transform = Some(session(Mode::Free));
        grab(&mut paint, Handle::Corner(1), [30.0, 10.0]);
        paint.transform_drag([40.0, 0.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert_eq!(quad[1], [40.0, 0.0]);
        assert_eq!(quad[0], [10.0, 10.0]);
    }

    #[test]
    fn ctrl_on_a_corner_distorts_in_scale_rotate() {
        let mut paint = SuePaint {
            transform: Some(session(Mode::ScaleRotate)),
            ..SuePaint::default()
        };
        grab(&mut paint, Handle::Corner(2), [30.0, 20.0]);
        paint.transform_drag([36.0, 28.0], Modifiers::COMMAND);
        let distorted = paint.transform.as_ref().unwrap();
        assert!(distorted.free, "the corners are free now");
        let quad = distorted.quad();
        assert_eq!(quad[2], [36.0, 28.0]);
        assert_eq!(quad[0], [10.0, 10.0]);
        assert_eq!(quad[1], [30.0, 10.0]);
    }

    #[test]
    fn the_rotation_centre_moves_and_the_box_turns_round_it() {
        let mut paint = SuePaint {
            transform: Some(session(Mode::ScaleRotate)),
            ..SuePaint::default()
        };
        // Its middle at first; dragged onto the top left corner.
        assert_eq!(
            paint.transform.as_ref().unwrap().pivot_point(),
            [20.0, 15.0]
        );
        grab(&mut paint, Handle::Pivot, [20.0, 15.0]);
        paint.transform_drag([10.0, 10.0], Modifiers::NONE);
        let moved = paint.transform.as_ref().unwrap();
        assert_eq!(moved.pivot, Pivot::Free);
        assert_eq!(moved.pivot_point(), [10.0, 10.0]);
        // A quarter turn about it keeps that corner where it is.
        grab(&mut paint, Handle::Rotate, [30.0, 10.0]);
        paint.transform_drag([10.0, 30.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert!(close(quad[0], [10.0, 10.0]), "{quad:?}");
        assert!(close(quad[1], [10.0, 30.0]), "{quad:?}");
        // A named point of the box: the bottom right corner.
        let mut corner = session(Mode::ScaleRotate);
        corner.pivot = Pivot::Point(2, 2);
        assert!(close(corner.pivot_point(), [30.0, 20.0]));
        corner.quarter_turn(true);
        assert!(close(corner.quad()[2], [30.0, 20.0]), "{:?}", corner.quad());
        // Shift turns in steps of 15 degrees.
        paint.transform = Some(session(Mode::ScaleRotate));
        grab(&mut paint, Handle::Rotate, [40.0, 15.0]);
        paint.transform_drag([40.0, 20.0], Modifiers::SHIFT);
        let angle = paint.transform.as_ref().unwrap().angle;
        assert!(
            (angle.to_degrees() - 15.0).abs() < 1e-3,
            "{}",
            angle.to_degrees()
        );
    }

    #[test]
    fn each_mode_drags_its_corner_its_own_way() {
        let mut paint = SuePaint {
            transform: Some(session(Mode::Skew)),
            ..SuePaint::default()
        };
        // Skew: the top edge slides along itself.
        grab(&mut paint, Handle::Corner(0), [10.0, 10.0]);
        paint.transform_drag([14.0, 13.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert!(
            close(quad[0], [14.0, 10.0]) && close(quad[1], [34.0, 10.0]),
            "{quad:?}"
        );
        // Perspective: the corners of the top edge move apart.
        paint.transform = Some(session(Mode::Perspective));
        grab(&mut paint, Handle::Corner(0), [10.0, 10.0]);
        paint.transform_drag([6.0, 10.0], Modifiers::NONE);
        let quad = paint.transform.as_ref().unwrap().quad();
        assert!(
            close(quad[0], [6.0, 10.0]) && close(quad[1], [34.0, 10.0]),
            "{quad:?}"
        );
        // Rotate: a corner turns the box about its middle.
        paint.transform = Some(session(Mode::Rotate));
        grab(&mut paint, Handle::Corner(1), [30.0, 10.0]);
        paint.transform_drag([30.0, 20.0], Modifiers::NONE);
        assert!(paint.transform.as_ref().unwrap().angle.abs() > 0.1);
        // Mode names come from the commands.
        assert_eq!(mode_of("edit.transform.skew"), Mode::Skew);
        assert_eq!(mode_of("edit.transform.scale"), Mode::ScaleRotate);
    }

    #[test]
    fn a_mesh_of_any_grid_moves_point_by_point() {
        let mut mesh = session(Mode::ScaleRotate);
        mesh.mesh = Some(Lattice::over(mesh.rect, 5, 3));
        let mut paint = SuePaint {
            transform: Some(mesh),
            ..SuePaint::default()
        };
        assert_eq!(
            paint.transform.as_ref().unwrap().points().unwrap().len(),
            15
        );
        grab(&mut paint, Handle::Point(7), [20.0, 15.0]);
        paint.transform_drag([22.0, 19.0], Modifiers::NONE);
        let points = paint.transform.as_ref().unwrap().points().unwrap();
        assert_eq!(points[7], [22.0, 19.0]);
        assert_eq!(points[0], [10.0, 10.0]);
        // Inside, away from the points, moves them all.
        grab(&mut paint, Handle::Move, [12.0, 12.0]);
        paint.transform_drag([13.0, 12.0], Modifiers::NONE);
        let moved = paint.transform.as_ref().unwrap().points().unwrap();
        assert_eq!(moved[0], [11.0, 10.0]);
        paint.transform.as_mut().unwrap().reset();
        assert_eq!(
            paint.transform.as_ref().unwrap().points().unwrap()[7],
            [20.0, 15.0]
        );
    }

    #[test]
    fn frames_are_grabbed_by_their_handles_and_scale_about_the_other_side() {
        let quad = [
            pos2(0.0, 0.0),
            pos2(100.0, 0.0),
            pos2(100.0, 50.0),
            pos2(0.0, 50.0),
        ];
        let (from, stalk) = stalk_of(&quad, 30.0);
        assert_eq!(from, pos2(50.0, 0.0));
        assert!((stalk.y + 30.0).abs() < 1e-3, "up from the top: {stalk:?}");
        assert_eq!(frame_grip(&quad, stalk, stalk, false), FrameGrip::Turn);
        assert_eq!(
            frame_grip(&quad, stalk, pos2(99.0, 49.0), false),
            FrameGrip::Corner(2)
        );
        assert_eq!(
            frame_grip(&quad, stalk, pos2(100.0, 26.0), false),
            FrameGrip::Edge(1)
        );
        assert_eq!(
            frame_grip(&quad, stalk, pos2(30.0, 30.0), true),
            FrameGrip::Inside
        );
        assert_eq!(
            frame_grip(&quad, stalk, pos2(300.0, 30.0), true),
            FrameGrip::Turn
        );
        assert_eq!(
            frame_grip(&quad, stalk, pos2(300.0, 30.0), false),
            FrameGrip::Inside
        );
        let box_: Quad = [[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let (held, anchor) = grip_points(&box_, FrameGrip::Corner(2), false).unwrap();
        assert_eq!((held, anchor), ([100.0, 50.0], [0.0, 0.0]));
        assert!((scale_ratio(anchor, held, [200.0, 100.0]) - 2.0).abs() < 1e-5);
        let (_, middle) = grip_points(&box_, FrameGrip::Edge(1), true).unwrap();
        assert_eq!(middle, [50.0, 25.0]);
        assert_eq!(constrained([5.0, 2.0], true), [5.0, 0.0]);
        assert_eq!(constrained([1.0, -3.0], true), [0.0, -3.0]);
        assert_eq!(constrained([1.0, -3.0], false), [1.0, -3.0]);
    }

    #[test]
    fn laying_over_mixes_straight_alpha() {
        let under = [0, 0, 255, 255, 0, 0, 0, 0];
        let top = [255, 0, 0, 255, 255, 0, 0, 0];
        assert_eq!(over(&under, &top), [255, 0, 0, 255, 0, 0, 0, 0]);
        let half = [255, 0, 0, 128, 0, 0, 0, 0];
        let mixed = over(&under, &half);
        assert_eq!(mixed[3], 255);
        assert!(mixed[0] > 120 && mixed[2] > 120);
    }
}
