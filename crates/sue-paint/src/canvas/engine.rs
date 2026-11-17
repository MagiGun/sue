//! The brush engine (`sue_brush::worker`) on the canvas: what Sue Paint
//! paints with (D2 of the 0.0.8 plan).
//!
//! The canvas only queues samples. Every packet of the pen's stream
//! (`sue_platform::stylus::take_pen_samples`: a WinTab packet, a Windows
//! Ink touch, a mouse move) goes to the stroke's worker with its own
//! place, time and pressure, mapped into the pixels of the raster the
//! stroke paints; the worker lays them on the shared pool. Where no sample
//! of the stream belongs to the stroke (a script driving the studio with
//! pointer events alone), the pointer's events are its samples. The
//! window's release is not a point of the line: it ends the stroke where
//! the pen last was.
//!
//! A plain stroke is shown by the compositor over its layer until the lift
//! merges it in as one undo step. A plain round tip's stroke on a layer
//! mask, inside a selection, on a layer with locked transparent pixels or
//! with symmetry's copies is laid straight into its rasters frame by
//! frame instead ([`super::lay`]), so it shows as it will stay.
//!
//! What the canvas adds to the samples: the rulers and the grid it snaps
//! to, Shift's straight line, post correction (the samples smoothed and
//! the stroke drawn again from them) and Smart Shape (the stroke drawn
//! again as the shape it was meant to be). A flick is the engine's own
//! (`sue_brush::input`): the canvas only turns it off for a straight line
//! and a line on a ruler or the grid.
//!
//! Every brush sub tool paints here. A plain round tip (the pens and the
//! pencils) is laid as coverage on the pool's threads, one worker for each
//! of symmetry's copies; every other brush (dabs that build up, mixing,
//! picture tips, paper, spray, color jitter) goes through its paint
//! behaviours on one worker that lays all its copies, keeps the layer's
//! locked pixels and stays inside the selection, and is shown as the
//! layer's pixels with the stroke on them. A stroke on a vector layer is
//! kept as a line of the engine: its samples go through a recorder on the
//! way to the worker, so the line paints again to the very bytes drawn.
//! The legacy engine (`sue_picture::brush::legacy`) only paints old
//! lines again.

use std::sync::Arc;

use egui::{Modifiers, Pos2, Rect};
use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::paint::Colors;
use sue_brush::paint::overflow::Region;
use sue_brush::raster::PixelRect;
use sue_brush::replay::{self, Recorder};
use sue_brush::worker::{Brush, Canvas, LiveView, StrokeWorker};
use sue_core::EntityId;
use sue_core::txn::Batch;
use sue_model::BrushPreset;
use sue_model::brush::{EngineKind, TipSource, param};
use sue_platform::stylus::{PenKind, PenSample, PenSource};
use sue_wm::{AppletContext, ArenaEdit};

use super::lay::{Lay, Target};
use crate::brush::{SAME, Tip};
use crate::doc;
use crate::rulers::Snap;
use crate::studio::{Gesture, Shaping, SuePaint};
use crate::tools::{SubTool, Tool};
use crate::vector::LineBrush;

/// How many of the pen's samples wait at most for a stroke to begin.
const PENDING: usize = 4096;

/// How far from the press a stream sample may be to be the press, in
/// screen points.
const NEAR: f32 = 24.0;

/// One copy of a stroke: the stroke itself, or one of symmetry's.
#[derive(Debug)]
struct Copy {
    worker: StrokeWorker,
    live: Arc<LiveView>,
}

/// What a stroke on a vector layer keeps to become its line.
#[derive(Debug)]
struct Keep {
    /// The brush, with its pictures, that the line names.
    brush: Arc<LineBrush>,
    /// The sub tool's tip, as the line's legacy tools read it.
    tip: Tip,
    /// The main and sub colors.
    colors: [[f32; 3]; 2],
    /// Every sample as the worker took it.
    recorder: Recorder,
}

/// A stroke of the engine under way.
#[derive(Debug)]
pub(crate) struct EngineStroke {
    copies: Vec<Copy>,
    brush: Brush,
    settings: Settings,
    /// Symmetry: each copy's map from the stroke's pixels to where it
    /// paints (the first copy is the stroke as drawn).
    transforms: Vec<[f32; 6]>,
    /// For a stroke laid through its paint behaviours: one worker lays
    /// every copy, on this canvas.
    canvas: Option<Canvas>,
    /// On a vector layer: what the stroke keeps to become a line.
    keep: Option<Keep>,
    /// The layer.
    layer: EntityId,
    /// The raster the stroke is drawn on: the layer's, or its mask's.
    target: EntityId,
    /// Where the stroke's pixel `(0, 0)` is in the picture.
    offset: [i64; 2],
    /// Laid straight into the rasters (see the module), or `None` when
    /// the compositor shows it until the lift.
    lay: Option<Lay>,
    label: String,
    eraser: bool,
    /// The stream source whose samples are the stroke's; `None` when the
    /// pointer's events are.
    source: Option<PenSource>,
    /// The last sample seen, in the stroke's pixels (after snapping).
    last: Sample,
    /// Every sample given to the worker, for drawing the stroke again.
    raw: Vec<Sample>,
    /// The lift has been given.
    lifted: bool,
    /// The ruler or grid line the stroke follows.
    snap: Option<Snap>,
    /// Screen points per layer pixel.
    zoom: f32,
    /// Shift: one straight line from the press, drawn on release.
    line: bool,
    /// Post correction, 0 for none.
    correction: u32,
    /// How post correction works (by speed, by scale, Bezier).
    correct: Correct,
    /// The brush sprays on while the pen holds still.
    continuous: bool,
    /// Where and since when (the frame clock, and the pen's own time) the
    /// pen has held still, for continuous spraying.
    held: Option<(f64, f64, [f32; 2])>,
    /// Smart Shape is on for this sub tool.
    smart: bool,
    /// Smart Shape: where and since when the pen has rested.
    rest: Option<(f64, Pos2)>,
    /// The shape the stroke became, while the pen is still down.
    shaping: Option<Shaping>,
}

/// The tool key of the built-in brushes of `tool` (`sue_brush::presets`).
fn brush_key(tool: Tool) -> Option<&'static str> {
    Some(match tool {
        Tool::Pen => "pen",
        Tool::Pencil => "pencil",
        Tool::Brush => "brush",
        Tool::Airbrush => "airbrush",
        Tool::Decoration => "decoration",
        Tool::Eraser => "eraser",
        Tool::Blend => "blend",
        _ => return None,
    })
}

/// `true` when `sub` is one of the Blend tool's blenders of the brush
/// engine (Blend, Painterly blender and the rest), which paint on the
/// engine; Blur, Finger Tip, Copy stamp and Liquify do not.
pub(crate) fn is_blender(sub: &SubTool) -> bool {
    sub.tool == Tool::Blend && crate::brush::engine::builtin("blend", &sub.name).is_some()
}

/// The brush a sub tool paints with on the engine, with its pictures
/// (`crate::brush::engine`): for a built-in sub tool as Sue ships it, its
/// own preset with every paint behaviour. On a mask (`round`) every brush
/// is drawn as its round tip, since a mask holds only how much shows.
/// `None` for a tool that does not paint.
pub(crate) fn brush_of(sub: &SubTool, eraser: bool, round: bool) -> Option<LineBrush> {
    let base = brush_key(sub.tool).and_then(|key| crate::brush::engine::builtin(key, &sub.name));
    if !sub.tool.paints() && !is_blender(sub) {
        return None;
    }
    let mut brush = crate::brush::engine::line_brush(&sub.tip, base.as_ref());
    let preset = &mut brush.preset;
    if round {
        *preset = round_of(preset);
        brush.pictures.clear();
    }
    if let Some(behaviours) = &sub.behaviours {
        apply_behaviours(preset, behaviours, sub.tip.grain);
    }
    if !round && let Some(rows) = &sub.brush_rows.preset {
        crate::brush_rows::apply_preset_rows(&mut brush, rows, &sub.tip);
    }
    // A paper made from a material travels with the brush's pictures.
    if let Some(sue_brush::paint::PaperChoice::Picture(hash)) =
        sue_brush::paint::Behaviours::from_preset(&brush.preset)
            .texture
            .map(|texture| texture.paper)
        && !brush.pictures.iter().any(|(known, _)| *known == hash)
        && let Some(picture) = crate::material_props::paper(&hash)
    {
        brush.pictures.push((hash, picture));
    }
    let preset = &mut brush.preset;
    preset.input.stabilization = sub.stabilization;
    preset.input.by_speed = sub.adjust_by_speed;
    preset.input.post_correction = sub.correction;
    preset.input.flick = sub.flick;
    preset.input.smart_shape = sub.smart_shape;
    preset.erase |= eraser;
    Some(brush)
}

/// The layers of `picture` besides `current` that the Multiple layers
/// eraser erases too, each with where its pixels sit from the stroke's
/// (`offset` is where the stroke's pixel `(0, 0)` is in the picture):
/// every raster layer that is shown, not locked, of 8-bit color and not a
/// vector layer. A layer in a hidden or locked folder is neither shown
/// nor open to edits, so it is left alone too.
fn erasable_layers(
    arena: &sue_core::Arena,
    picture: EntityId,
    current: EntityId,
    offset: [i64; 2],
) -> Vec<(EntityId, [i64; 2])> {
    let all = doc::layers(arena, picture);
    let closed: Vec<EntityId> = all
        .iter()
        .filter(|info| doc::is_folder(arena, info.id) && (!info.style.visible || info.locked))
        .map(|info| info.id)
        .collect();
    all.into_iter()
        .filter(|info| {
            info.id != current
                && info.kind == sue_model::picture::LayerKind::Raster
                && info.style.visible
                && !info.locked
                && !doc::folders_around(arena, info.id)
                    .iter()
                    .any(|folder| closed.contains(folder))
                && !crate::vector::is_vector(arena, info.id)
                && doc::raster(arena, info.id)
                    .is_some_and(|raster| raster.format() == sue_core::PixelFormat::Rgba8)
        })
        .map(|info| {
            let delta = [
                info.style.offset[0] - offset[0],
                info.style.offset[1] - offset[1],
            ];
            (info.id, delta)
        })
        .collect()
}

/// The region anti-overflow `o` lets a stroke on `layer` (whose pixel
/// `(0, 0)` is picture pixel `offset`) paint in, pressed at layer pixel
/// `at`: what the lines of the shown reference layers of `picture` close
/// around it, grown or drawn back by its scaling. `None` while the
/// picture has no reference layer.
fn anti_overflow(
    arena: &sue_core::Arena,
    picture: EntityId,
    layer: EntityId,
    offset: [i64; 2],
    at: [f32; 2],
    o: sue_picture::tools::brush_rows::AntiOverflow,
) -> Option<Region> {
    let layers = doc::layers(arena, picture);
    if !layers.iter().any(|info| doc::is_reference(arena, info.id)) {
        return None;
    }
    let size = sue_model::picture::size(arena.get_by_id(picture).ok()?)?;
    let pixels = sue_picture::fill::reference(
        arena,
        picture,
        size,
        Some(layer),
        &[],
        sue_picture::fill::Refer::Reference,
        sue_picture::fill::Exclude::default(),
    )?;
    let reference =
        sue_core::RasterTile2D::from_bytes(size[0], size[1], sue_core::PixelFormat::Rgba8, pixels)
            .ok()?;
    let origin = [-offset[0] as i32, -offset[1] as i32];
    let start = [at[0].floor() as i32, at[1].floor() as i32];
    let region = Region::enclosing(&reference, origin, start, o.tolerance.clamp(0.01, 1.0))?;
    Some(region.scaled(o.scaling))
}

/// `preset` with the paint behaviours Sub Tool Detail set. The texture's
/// strength stays the tip's Paper grain (`grain`, 0 to 1), the row
/// Tool Property has always shown for it; a paper chosen where there was
/// none takes it at full strength while the grain is still 0.
pub(crate) fn apply_behaviours(
    preset: &mut BrushPreset,
    behaviours: &sue_brush::paint::Behaviours,
    grain: f32,
) {
    let had = preset.texture.is_some();
    behaviours.write_to(preset);
    if preset.texture.is_some() {
        let strength = if had || grain > 0.0 { grain } else { 1.0 };
        preset.set(param::TEXTURE_STRENGTH, strength);
    }
}

/// The paint behaviours `sub` paints with now: what Sub Tool Detail set,
/// else its brush's own.
pub(crate) fn behaviours_of(sub: &SubTool) -> sue_brush::paint::Behaviours {
    sub.behaviours.clone().unwrap_or_else(|| {
        brush_of(sub, false, false).map_or_else(sue_brush::paint::Behaviours::default, |brush| {
            sue_brush::paint::Behaviours::from_preset(&brush.preset)
        })
    })
}

/// `preset` as its round tip alone (on a mask).
fn round_of(preset: &BrushPreset) -> BrushPreset {
    let mut preset = preset.clone();
    preset.engine = EngineKind::Stroke;
    preset.tip = TipSource::Round;
    // Round: a flat tip is round too.
    preset.params.remove(sue_model::brush::param::ASPECT);
    preset
        .dynamics
        .retain(|binding| binding.param != sue_model::brush::param::ASPECT);
    preset.mix = None;
    preset.texture = None;
    preset.build_up = false;
    preset.blend = sue_model::BlendMode::Normal;
    preset.color = sue_model::brush::ColorJitter::default();
    preset
        .extra
        .retain(|(key, _)| key.starts_with(BrushPreset::LEGACY));
    preset
}

impl EngineStroke {
    /// The workers for every copy of `brush` drawn as `settings` say: one
    /// for all of them on `canvas` (paint behaviours), else one each.
    fn start(
        brush: &Brush,
        settings: Settings,
        transforms: &[[f32; 6]],
        canvas: Option<&Canvas>,
    ) -> Vec<Copy> {
        let pool = crate::composite::share_brush_pool();
        if let Some(canvas) = canvas {
            let worker = StrokeWorker::start_on(pool, brush, settings, canvas.clone());
            let live = worker.live();
            return vec![Copy { worker, live }];
        }
        transforms
            .iter()
            .map(|m| {
                let mut settings = settings;
                settings.to_layer = [[m[0], m[1], m[4]], [m[2], m[3], m[5]]];
                let worker = StrokeWorker::start(Arc::clone(&pool), brush, settings);
                let live = worker.live();
                Copy { worker, live }
            })
            .collect()
    }

    /// The live views of every copy.
    fn lives(&self) -> Vec<Arc<LiveView>> {
        self.copies
            .iter()
            .map(|copy| Arc::clone(&copy.live))
            .collect()
    }

    /// Gives `sample` to every copy (on a vector layer, as its recorder
    /// keeps it, so the line paints again to the same bytes).
    fn send(&mut self, sample: Sample) {
        let given = match self.keep.as_mut() {
            Some(keep) => keep.recorder.take(sample),
            None => Some(sample),
        };
        if let Some(given) = given {
            for copy in &mut self.copies {
                copy.worker.push(given);
            }
        }
        self.raw.push(sample);
        if sample.kind == Kind::Up {
            self.lifted = true;
        }
    }

    /// One sample of the pen, in the stroke's pixels: snapped to its ruler,
    /// held back for Shift's line and for a stroke that became its shape,
    /// and given to the workers.
    fn feed(&mut self, mut sample: Sample) {
        if self.lifted {
            return;
        }
        if let Some(snap) = self.snap.as_mut() {
            match snap.follow(sample.pos, 4.0 / self.zoom.max(0.01)) {
                Some(on) => sample.pos = on,
                // Following a ruler: nothing is drawn until the pen shows
                // which way it goes.
                None if sample.kind == Kind::Move => return,
                None => {}
            }
        }
        if sample.kind == Kind::Up {
            self.lift(sample.pos, sample.time);
            return;
        }
        if sample.pressure.is_none() {
            sample.pressure = self.last.pressure;
        }
        self.last = sample;
        if sample.kind == Kind::Move && (self.line || self.shaping.is_some()) {
            // A straight line waits for the release; a stroke that
            // became its shape stays it.
            return;
        }
        self.send(sample);
    }

    /// The pen lifted at `at` (the stroke's pixels) at device time `time`:
    /// Shift's line is drawn to it, and the lift is given (a quick lift
    /// flicks into a point in the engine, as the brush says).
    fn lift(&mut self, at: [f32; 2], time: f64) {
        if self.lifted {
            return;
        }
        let time = time.max(self.last.time);
        if self.shaping.is_some() {
            // The shape ends where it is drawn to (the shape as the pen
            // last dragged it).
            let end = self.raw.last().map_or(at, |sample| sample.pos);
            self.send(Sample::new(Kind::Up, end, time));
            return;
        }
        let end = at;
        if self.line || at != self.last.pos {
            // Shift's line: from the press straight to the lift; else the
            // pen went on to where it was let go (a pointer let go past
            // its last move).
            let mut last = self.last;
            last.kind = Kind::Move;
            last.pos = at;
            last.time = time;
            self.send(last);
        }
        let time = self.raw.last().map_or(time, |sample| sample.time.max(time));
        self.send(Sample::new(Kind::Up, end, time));
    }

    /// `true` once the stroke is on a ruler or grid line.
    fn snapped(&self) -> bool {
        self.snap.as_ref().is_some_and(Snap::chosen)
    }

    /// The brush's size in layer pixels.
    fn size(&self) -> f32 {
        self.brush
            .preset
            .value(param::SIZE)
            .unwrap_or(10.0)
            .max(0.5)
    }

    /// The samples smoothed by post correction, `level` 1 to 10 (see
    /// [`corrected`]).
    fn corrected(&self) -> Vec<Sample> {
        corrected_as(&self.raw, self.correction, self.size(), self.correct)
    }
}

/// How post correction works, beside its level (full guide 00:33:03,
/// FGV-29).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Correct {
    /// A quicker stroke is smoothed more, a slow one less.
    pub by_speed: bool,
    /// The smoothing is measured on the screen, so it looks the same at
    /// any zoom.
    pub by_scale: bool,
    /// The smoothed line is laid again as Bezier curves through as few
    /// points as keep it, so it runs in long clean arcs.
    pub bezier: bool,
    /// Screen points per layer pixel.
    pub zoom: f32,
}

impl Default for Correct {
    fn default() -> Self {
        Self {
            by_speed: false,
            by_scale: false,
            bezier: false,
            zoom: 1.0,
        }
    }
}

/// How fast a stroke of `samples` went on the screen, in points a
/// second, `zoom` screen points to a layer pixel.
fn screen_speed(samples: &[Sample], zoom: f32) -> f32 {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else {
        return 0.0;
    };
    let length: f32 = samples
        .windows(2)
        .map(|pair| (pair[1].pos[0] - pair[0].pos[0]).hypot(pair[1].pos[1] - pair[0].pos[1]))
        .sum();
    let seconds = (last.time - first.time).max(1e-3) as f32;
    length * zoom / seconds
}

/// [`corrected`] as `how` says: by speed it smooths a quick stroke up to
/// twice as much and a slow one down to half (a stroke of 800 screen
/// points a second is as the level says); by scale it smooths as far on
/// the screen whatever the zoom and the brush's size; Bezier lays the
/// result again as curves through the fewest points that keep it.
pub(crate) fn corrected_as(samples: &[Sample], level: u32, size: f32, how: Correct) -> Vec<Sample> {
    if level == 0 {
        return samples.to_vec();
    }
    let zoom = how.zoom.max(0.01);
    let mut size = size;
    if how.by_scale {
        // The smoothing reaches as far on the screen at any zoom and for
        // any brush: as for a brush 10 points wide on the screen.
        size = 10.0 / zoom;
    }
    if how.by_speed {
        let speed = screen_speed(samples, zoom);
        size *= (speed / 800.0).clamp(0.5, 2.0);
    }
    let out = corrected(samples, level, size);
    if how.bezier {
        let tolerance = (level.min(10) as f32 * 0.5 + 1.0) * (size * 0.12).max(1.0);
        bezier_fit(&out, tolerance)
    } else {
        out
    }
}

/// `samples` laid again on cubic Bezier curves through the fewest of
/// them that keep the line within `tolerance` layer pixels: the kept
/// points are the curve's anchors, their handles a third of the way to
/// the neighbours along the line's direction (Catmull-Rom), and every
/// other sample moves onto the curve at its share of the way. Ends,
/// pressure and time stay.
pub(crate) fn bezier_fit(samples: &[Sample], tolerance: f32) -> Vec<Sample> {
    let body: Vec<usize> = (0..samples.len())
        .filter(|&i| samples[i].kind != Kind::Up)
        .collect();
    if body.len() < 4 {
        return samples.to_vec();
    }
    let pos = |i: usize| samples[body[i]].pos;
    // Douglas and Peucker: keep the farthest point while any is too far.
    let mut keep = vec![false; body.len()];
    keep[0] = true;
    keep[body.len() - 1] = true;
    let mut spans = vec![(0, body.len() - 1)];
    while let Some((a, b)) = spans.pop() {
        if b <= a + 1 {
            continue;
        }
        let (p, q) = (pos(a), pos(b));
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let length = dx.hypot(dy);
        let far = (a + 1..b)
            .map(|i| {
                let r = pos(i);
                let d = if length < 1e-4 {
                    (r[0] - p[0]).hypot(r[1] - p[1])
                } else {
                    ((r[0] - p[0]) * dy - (r[1] - p[1]) * dx).abs() / length
                };
                (i, d)
            })
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((i, d)) = far
            && d > tolerance
        {
            keep[i] = true;
            spans.push((a, i));
            spans.push((i, b));
        }
    }
    let anchors: Vec<usize> = (0..body.len()).filter(|&i| keep[i]).collect();
    let mut out = samples.to_vec();
    for (k, pair) in anchors.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let before = if k == 0 { a } else { anchors[k - 1] };
        let after = anchors.get(k + 2).copied().unwrap_or(b);
        let (p0, p3) = (pos(a), pos(b));
        let (pm, pn) = (pos(before), pos(after));
        let p1 = [p0[0] + (p3[0] - pm[0]) / 6.0, p0[1] + (p3[1] - pm[1]) / 6.0];
        let p2 = [p3[0] - (pn[0] - p0[0]) / 6.0, p3[1] - (pn[1] - p0[1]) / 6.0];
        // Each sample between takes its share of the way along the line.
        let mut along = vec![0.0_f32];
        for i in a + 1..=b {
            let (u, v) = (pos(i - 1), pos(i));
            along.push(along[along.len() - 1] + (v[0] - u[0]).hypot(v[1] - u[1]));
        }
        let total = along[along.len() - 1].max(1e-4);
        for i in a + 1..b {
            let t = along[i - a] / total;
            let m = 1.0 - t;
            let w = [m * m * m, 3.0 * m * m * t, 3.0 * m * t * t, t * t * t];
            out[body[i]].pos = [
                w[0] * p0[0] + w[1] * p1[0] + w[2] * p2[0] + w[3] * p3[0],
                w[0] * p0[1] + w[1] * p1[1] + w[2] * p2[1] + w[3] * p3[1],
            ];
        }
    }
    out
}

/// Post correction: `samples` (a whole stroke, its lift last) with their
/// places smoothed along the line, as much as `level` (1 to 10) says for
/// a brush `size` wide. Its ends and its sharp corners stay where they
/// are, so a corner is not rounded off and the line is not pulled in at
/// its ends; pressure and time stay with each sample.
pub(crate) fn corrected(samples: &[Sample], level: u32, size: f32) -> Vec<Sample> {
    let count = samples.len();
    if level == 0 || count < 5 {
        return samples.to_vec();
    }
    // How far along the line each sample is.
    let mut along = Vec::with_capacity(count);
    let mut total = 0.0_f32;
    for (i, sample) in samples.iter().enumerate() {
        if i > 0 {
            let p = samples[i - 1].pos;
            total += (sample.pos[0] - p[0]).hypot(sample.pos[1] - p[1]);
        }
        along.push(total);
    }
    let sigma = level.min(10) as f32 * 1.2 * (size * 0.12).max(1.0);
    // Corners: the line turns more than about 65 degrees between arms a
    // brush wide on either side.
    let arm = size.max(6.0);
    let place_at = |s: f32| -> [f32; 2] {
        let i = along.partition_point(|&a| a < s).min(count - 1);
        samples[i].pos
    };
    let turn = |i: usize| -> f32 {
        let (s, b) = (along[i], samples[i].pos);
        if s < arm || total - s < arm {
            return 0.0;
        }
        let (a, c) = (place_at(s - arm), place_at(s + arm));
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let (lu, lv) = (u[0].hypot(u[1]), v[0].hypot(v[1]));
        if lu < 1e-3 || lv < 1e-3 {
            return 0.0;
        }
        ((u[0] * v[0] + u[1] * v[1]) / (lu * lv))
            .clamp(-1.0, 1.0)
            .acos()
    };
    let turns: Vec<f32> = (0..count).map(turn).collect();
    let mut anchors = vec![0.0, total];
    for i in 0..count {
        if turns[i] > 1.13 {
            let (s, t) = (along[i], turns[i]);
            let sharpest = (0..count)
                .filter(|&j| (along[j] - s).abs() <= arm)
                .all(|j| turns[j] <= t);
            if sharpest {
                anchors.push(s);
            }
        }
    }
    let mut out = samples.to_vec();
    for i in 0..count {
        let s = along[i];
        // As close to an anchor as a third of the way to it: an anchor
        // stays put, and nothing is pulled across it.
        let near = anchors
            .iter()
            .map(|a| (a - s).abs())
            .fold(f32::INFINITY, f32::min);
        let local = sigma.min(near / 3.0);
        if local < 0.05 || samples[i].kind == Kind::Up {
            continue;
        }
        let (mut sum, mut weight) = ([0.0_f32; 2], 0.0_f32);
        for (j, sample) in samples.iter().enumerate() {
            let d = along[j] - s;
            if d.abs() > local * 3.0 {
                continue;
            }
            let w = (-(d * d) / (2.0 * local * local)).exp();
            sum[0] += sample.pos[0] * w;
            sum[1] += sample.pos[1] * w;
            weight += w;
        }
        if weight > 0.0 {
            out[i].pos = [sum[0] / weight, sum[1] / weight];
        }
    }
    // The lift ends where the line now does.
    let end = out
        .iter()
        .rev()
        .find(|sample| sample.kind != Kind::Up)
        .map(|sample| sample.pos);
    if let (Some(pos), Some(last)) = (end, out.last_mut())
        && last.kind == Kind::Up
    {
        last.pos = pos;
    }
    out
}

impl SuePaint {
    /// Takes this frame's pen samples from the stream and keeps those
    /// since the latest touch down, for a stroke that starts with it (at
    /// most [`PENDING`], the newest). `true` when a pen said how hard it
    /// pressed.
    pub(super) fn engine_take_samples(&mut self) -> bool {
        let mut pressed = false;
        for sample in sue_platform::stylus::take_pen_samples() {
            if sample.kind == PenKind::Down && !matches!(self.gesture, Some(Gesture::Engine(_))) {
                self.pen_stream.clear();
            }
            pressed |= sample.source.is_pen() && sample.pressure.is_some();
            if self.pen_stream.len() >= PENDING {
                self.pen_stream.drain(..PENDING / 2);
            }
            self.pen_stream.push(sample);
        }
        pressed
    }

    /// Where screen point `pos` is in pixels that sit at `offset` in the
    /// picture.
    fn layer_point(&self, pos: Pos2, viewport: Rect, size: [u32; 2], offset: [i64; 2]) -> [f32; 2] {
        let Some(doc) = self.docs.get(self.active) else {
            return [pos.x, pos.y];
        };
        let p = doc.view.to_picture(viewport, size, pos);
        [p.x - offset[0] as f32, p.y - offset[1] as f32]
    }

    /// A stream sample as the worker takes it.
    fn engine_sample(
        &self,
        sample: &PenSample,
        viewport: Rect,
        size: [u32; 2],
        offset: [i64; 2],
    ) -> Sample {
        let kind = match sample.kind {
            PenKind::Down => Kind::Down,
            PenKind::Move => Kind::Move,
            PenKind::Up => Kind::Up,
        };
        let screen = Pos2::new(sample.pos[0], sample.pos[1]);
        let pos = self.layer_point(screen, viewport, size, offset);
        // The pen's lean turns and mirrors with the view.
        let tilt = sample.tilt.map(|tilt| match self.docs.get(self.active) {
            Some(doc) => super::input::lean_in_picture(Some(tilt), screen, |at| {
                doc.view.to_picture(viewport, size, at)
            }),
            None => tilt,
        });
        // And so does its twist.
        let twist = sample.twist.map(|twist| match self.docs.get(self.active) {
            Some(doc) => super::input::twist_in_picture(twist, screen, |at| {
                doc.view.to_picture(viewport, size, at)
            }),
            None => twist,
        });
        Sample {
            pos,
            time: sample.time,
            pressure: sample.pressure,
            tilt,
            twist,
            kind,
        }
    }

    /// Starts a stroke of the engine for `sub` at `pos`. `false`, with a
    /// notice saying why, when the layer takes no stroke.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn engine_press(
        &mut self,
        pos: Pos2,
        sub: &SubTool,
        modifiers: Modifiers,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let Some(doc) = self.docs.get(self.active) else {
            return false;
        };
        let info = match self.mask_drawable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return false;
            }
        };
        let arena = &cx.world.arena;
        // Editing the mask: the pen shows the layer, the eraser hides it,
        // in the mask's own pixels where the mask sits (a vector layer's
        // mask too: it is gray pixels like any other).
        let mask = doc
            .mask_edit
            .then(|| doc::mask_of(arena, info.id))
            .flatten()
            .zip(doc::mask_offset(arena, info.id));
        let vector = mask.is_none() && crate::vector::is_vector(arena, info.id);
        // Drawing in the transparent color erases; a blender has no
        // color of its own to make transparent.
        let transparent = self.transparent && sub.tool != Tool::Blend;
        let Some(line_brush) = brush_of(sub, transparent, mask.is_some()) else {
            return false;
        };
        let (target, offset) = mask.unwrap_or((info.id, info.style.offset));
        let fits = doc::raster(arena, target).is_some_and(|raster| {
            raster.bytes().is_some()
                && match raster.format() {
                    sue_core::PixelFormat::Rgba8 => true,
                    sue_core::PixelFormat::Gray8 => mask.is_some(),
                    sue_core::PixelFormat::RgbaF32 => false,
                }
        });
        if !fits {
            cx.fail(tr!("layer-takes-no-strokes", name = info.name));
            return false;
        }
        let zoom = doc.view.zoom;
        let main = if mask.is_some() {
            [1.0; 3]
        } else {
            crate::color::unit(self.drawing_color())
        };
        let other = crate::color::unit(if self.sub_active { self.main } else { self.sub });
        let eraser = line_brush.preset.erase;
        let local = self.layer_point(pos, viewport, size, offset);
        // The picture's rulers, in the stroke's pixels.
        let rulers = if doc.show_rulers {
            crate::rulers::Rulers::of(arena, doc.picture)
                .shifted([-(offset[0] as f32), -(offset[1] as f32)])
        } else {
            crate::rulers::Rulers::default()
        };
        // Snap to Special Ruler, and the picture's own switch.
        // A sub tool with Enable snapping off draws freely past every
        // ruler and the grid (full guide 00:33:16).
        let snapping = sub.brush_rows.snapping;
        let special =
            snapping && doc.ruler_on && crate::commands::special_snaps(arena, doc.picture);
        let mut transforms = vec![SAME];
        if let Some((center, angle)) = rulers.symmetry.filter(|_| special)
            && let Some(ruler) = self
                .toolbox
                .subs(Tool::Ruler)
                .iter()
                .find(|sub| sub.mode == 0)
        {
            // The symmetrical ruler: a copy through every line.
            transforms = crate::brush::symmetry(
                center,
                angle,
                crate::rulers::turns(ruler.lines, ruler.mirror),
                ruler.mirror,
            );
        }
        // A stroke that starts near a ruler goes onto it.
        let snap = Snap::begin(
            &rulers,
            local,
            doc.snap_ruler && snapping,
            special,
            24.0 / zoom.max(0.01),
        )
        .or_else(|| {
            (self.snap_grid && snapping)
                .then(|| {
                    // The finest lines of the picture's grid, through its
                    // origin.
                    let grid = crate::grid::Grid::of(arena, doc.picture);
                    let origin = crate::grid::origin_of(arena, doc.picture, size);
                    Snap::grid(
                        local,
                        [offset[0] as f32 - origin[0], offset[1] as f32 - origin[1]],
                        grid.step(),
                        12.0 / zoom.max(0.01),
                    )
                })
                .flatten()
        });
        // Preferences, Tools: Shift draws a straight line.
        let shift_line =
            modifiers.shift && cx.rice().get_bool("sue_paint.shift_line").unwrap_or(true);
        // A straight line, or one along a ruler, does not flick.
        let mut line_brush = line_brush;
        line_brush.preset.input.flick &= !shift_line && snap.is_none();
        // The Multiple layers eraser: a plain eraser laid straight into
        // every layer it can erase (`all_layers`), the stroke itself plain.
        let others = if mask.is_none() && !vector && eraser {
            if sue_brush::paint::Behaviours::from_preset(&line_brush.preset).all_layers {
                line_brush
                    .preset
                    .extra
                    .retain(|(key, _)| key != sue_brush::paint::field::ALL_LAYERS);
                erasable_layers(arena, doc.picture, info.id, offset)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let line_brush = Arc::new(line_brush);
        let mut brush = Brush::new(line_brush.preset.clone(), main)
            .with(crate::brush::engine::resources(&line_brush), other);
        // Anti-overflow: the paint stays inside what the reference
        // layer's lines close around the press (FGV-31).
        let overflow = sub
            .brush_rows
            .overflow
            .filter(|_| mask.is_none() && !vector)
            .and_then(|o| anti_overflow(arena, doc.picture, info.id, offset, local, o));
        let fenced = overflow.is_some();
        if let Some(region) = overflow {
            brush.resources.overflow = Some(Arc::new(region));
        }
        // The layer's transparent pixels locked: paint only where it has
        // some (an eraser still erases). A vector line keeps its own.
        let lock = mask.is_none() && info.alpha_locked && !eraser && !vector;
        // Vector lines are kept whole, not cut by the selection.
        let clip = doc
            .selection
            .clone()
            .filter(|_| !vector)
            .map(|selection| (selection, offset));
        let behaviours = vector || fenced || !brush.is_plain();
        let (lay, canvas) = if behaviours {
            // One worker lays every copy, keeps the locked pixels and the
            // selection, on a snapshot of the layer.
            let region = clip
                .as_ref()
                .map(|(selection, offset)| Arc::new(selection_region(selection, *offset)));
            let canvas = Canvas {
                snapshot: doc::raster(arena, target).cloned(),
                keep_alpha: lock,
                copies: if transforms.len() > 1 {
                    transforms.clone()
                } else {
                    Vec::new()
                },
                replay: vector,
            };
            let mut brush = brush.clone();
            brush.resources.overflow = match (brush.resources.overflow.take(), region) {
                (Some(own), Some(region)) => Some(Arc::new(own.and(&region))),
                (own, region) => own.or(region),
            };
            (None, Some((canvas, brush)))
        } else {
            let mut targets = vec![Target::new(target, [0, 0], lock)];
            let direct = mask.is_some()
                || clip.is_some()
                || lock
                || transforms.len() > 1
                || !others.is_empty();
            targets.extend(
                others
                    .iter()
                    .map(|&(layer, delta)| Target::new(layer, delta, false)),
            );
            (direct.then(|| Lay::new(brush.ink(), clip, targets)), None)
        };
        let (brush, canvas) = match canvas {
            Some((canvas, brush)) => (brush, Some(canvas)),
            None => (brush, None),
        };
        // The press is the stream's latest touch down near it, with the
        // samples after it; else the pointer's events are the samples.
        let down = self.pen_stream.iter().rposition(|sample| {
            sample.kind == PenKind::Down
                && Pos2::new(sample.pos[0], sample.pos[1]).distance(pos) <= NEAR
        });
        let mut settings = Settings::new(Device::Pen);
        settings.scale = zoom;
        let mut first = Vec::new();
        let source = match down {
            Some(at) => {
                let source = self.pen_stream[at].source;
                if source == PenSource::Mouse {
                    settings.device = Device::MOUSE;
                } else if self.pen_stream[at].pressure.is_none() {
                    // A finger on a screen that does not feel force.
                    settings.device = Device::Constant(1.0);
                }
                settings.curve = self.pen_curve;
                let stream = std::mem::take(&mut self.pen_stream);
                first.extend(
                    stream[at..]
                        .iter()
                        .filter(|sample| sample.source == source)
                        .map(|sample| self.engine_sample(sample, viewport, size, offset)),
                );
                Some(source)
            }
            None => {
                // The pointer's own events: a touch that said how hard it
                // pressed (through the pen pressure curve already), else a
                // mouse, which presses evenly.
                let mut down = Sample::new(Kind::Down, local, self.clock.0);
                match self.pressure {
                    Some(pressure) => down.pressure = Some(pressure),
                    None => settings.device = Device::MOUSE,
                }
                first.push(down);
                None
            }
        };
        settings.seed = self.clock.0.to_bits();
        let keep = vector.then(|| Keep {
            brush: Arc::clone(&line_brush),
            tip: sub.tip,
            colors: [main, other],
            recorder: Recorder::new(
                replay::ENGINE,
                line_brush.hash(),
                &settings,
                Colors { main, sub: other },
                canvas
                    .as_ref()
                    .map(|canvas| canvas.copies.clone())
                    .unwrap_or_default(),
            ),
        });
        let copies = EngineStroke::start(&brush, settings, &transforms, canvas.as_ref());
        let mut stroke = EngineStroke {
            copies,
            brush,
            settings,
            transforms,
            canvas,
            keep,
            layer: info.id,
            target,
            offset,
            lay,
            label: crate::localize(&sub.name),
            eraser,
            source,
            last: Sample::new(Kind::Down, local, self.clock.0),
            raw: Vec::new(),
            lifted: false,
            snap,
            zoom,
            line: shift_line,
            correction: sub.correction,
            continuous: sue_brush::paint::Behaviours::from_preset(&line_brush.preset)
                .continuous
                .is_some(),
            held: None,
            correct: Correct {
                by_speed: sub.brush_rows.correction_by_speed,
                by_scale: sub.brush_rows.correction_by_scale,
                bezier: sub.brush_rows.bezier,
                zoom,
            },
            smart: sub.smart_shape,
            rest: None,
            shaping: None,
        };
        for sample in first {
            stroke.feed(sample);
        }
        if stroke.lay.is_none()
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            let shown: Arc<dyn crate::composite::LiveStroke> = Arc::new(
                crate::composite::BrushLive::new(info.id, Arc::clone(&stroke.copies[0].live)),
            );
            doc.compositor.set_live(&cx.world.arena, Some(shown));
        }
        crate::studio::saw_stroke(crate::studio::StrokeSeen {
            sub: sub.name.clone(),
            behaviours,
            vector,
        });
        self.gesture = Some(Gesture::Engine(Box::new(stroke)));
        true
    }

    /// The pointer moved to `pos` during a stroke drawn from the
    /// pointer's events: one more sample (or, past a ruler's choosing,
    /// where the pen is for Shift's line and Smart Shape).
    pub(super) fn engine_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let pressure = self.pressure;
        let time = self.clock.0;
        let Some(Gesture::Engine(stroke)) = &self.gesture else {
            return;
        };
        if stroke.source.is_some() || stroke.lifted {
            return;
        }
        let at = self.layer_point(pos, viewport, size, stroke.offset);
        let mut sample = Sample::new(Kind::Move, at, time);
        sample.pressure = pressure;
        if let Some(Gesture::Engine(stroke)) = &mut self.gesture {
            stroke.feed(sample);
        }
    }

    /// Hands the stroke this frame's samples of its source.
    fn engine_feed(&mut self, viewport: Rect, size: [u32; 2]) {
        let Some(Gesture::Engine(stroke)) = &self.gesture else {
            return;
        };
        let (Some(source), offset) = (stroke.source, stroke.offset) else {
            return;
        };
        let stream = std::mem::take(&mut self.pen_stream);
        let samples: Vec<Sample> = stream
            .iter()
            .filter(|sample| sample.source == source)
            .map(|sample| self.engine_sample(sample, viewport, size, offset))
            .collect();
        let Some(Gesture::Engine(stroke)) = &mut self.gesture else {
            return;
        };
        for sample in samples {
            stroke.feed(sample);
        }
    }

    /// Once a frame: the frame's samples to the workers, and what they
    /// laid since the last frame shown.
    pub(super) fn engine_frame(
        &mut self,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        self.engine_feed(viewport, size);
        let (visible, step) = self.on_screen;
        let now = self.clock.0;
        let Some(Gesture::Engine(stroke)) = &mut self.gesture else {
            return;
        };
        // Continuous spraying (full guide 00:31:12): while the pen holds
        // still the brush keeps laying, so each frame says where it still
        // is and when.
        // Its time goes on from the pen's own clock by the frames' time
        // since the pen last said anything.
        if stroke.continuous && !stroke.lifted && !stroke.line {
            let last = stroke.last;
            match stroke.held {
                Some((clock, time, pos)) if time == last.time && pos == last.pos => {
                    if now > clock {
                        let mut still = last;
                        still.kind = Kind::Move;
                        still.time = time + (now - clock);
                        stroke.feed(still);
                        stroke.held = Some((clock, time, pos));
                        // `feed` moved `last` on: the hold goes on from
                        // the pen's own sample.
                        stroke.last = last;
                    }
                }
                _ => stroke.held = Some((now, last.time, last.pos)),
            }
        }
        let mut dirty: Vec<_> = stroke
            .copies
            .iter_mut()
            .flat_map(|copy| copy.worker.poll())
            .collect();
        if dirty.is_empty() {
            return;
        }
        dirty.sort_unstable();
        dirty.dedup();
        let (layer, offset) = (stroke.layer, stroke.offset);
        let lives = stroke.lives();
        let rects = match stroke.lay.as_mut() {
            Some(lay) => lay.lay(&mut cx.world.arena, &dirty, &lives, offset),
            None => tile_rects(dirty.iter().map(|key| key.origin()), offset),
        };
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh_view(&cx.world.arena, &rects, Some(layer), visible, step);
        }
    }

    /// The button was let go at `pos`: the stroke ends where the pen last
    /// was, and goes into its layer as one undo step.
    pub(super) fn engine_release(
        &mut self,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        self.engine_feed(viewport, size);
        let Some(Gesture::Engine(mut stroke)) = self.gesture.take() else {
            return;
        };
        if !stroke.lifted {
            // A lift says only where the line ends: where the pen was
            // last seen, or for pointer events where the button went.
            let mut at = match stroke.source {
                Some(_) => stroke.last.pos,
                None => self.layer_point(pos, viewport, size, stroke.offset),
            };
            if let Some(snap) = stroke.snap.as_mut() {
                at = snap
                    .follow(at, 4.0 / stroke.zoom.max(0.01))
                    .unwrap_or(stroke.last.pos);
            }
            let time = stroke.last.time.max(self.clock.0);
            stroke.lift(at, time);
        }
        // Post correction: the stroke drawn again from its samples,
        // smoothed. Not for a straight line, a line on a ruler, or a
        // stroke that became its shape.
        if stroke.correction > 0 && !stroke.line && !stroke.snapped() && stroke.shaping.is_none() {
            let samples = stroke.corrected();
            self.engine_redraw(&mut stroke, samples, cx);
        }
        self.engine_end(*stroke, cx);
    }

    /// Draws `stroke` again from `samples` alone: what it showed or laid
    /// so far is taken back first.
    fn engine_redraw(
        &mut self,
        stroke: &mut EngineStroke,
        samples: Vec<Sample>,
        cx: &mut AppletContext<'_>,
    ) {
        let (visible, step) = self.on_screen;
        let plain = stroke.lay.is_none();
        let mut rects = Vec::new();
        if let Some(lay) = stroke.lay.as_mut() {
            rects = lay.restore(&mut cx.world.arena, stroke.offset);
        } else if let Some(doc) = self.docs.get_mut(self.active) {
            // Let go while it still says what it covered, so that is shown
            // again from the layer.
            doc.compositor.set_live(&cx.world.arena, None);
        }
        let fresh = EngineStroke::start(
            &stroke.brush,
            stroke.settings,
            &stroke.transforms,
            stroke.canvas.as_ref(),
        );
        for old in std::mem::replace(&mut stroke.copies, fresh) {
            old.worker.abandon();
        }
        if let Some(keep) = stroke.keep.as_mut() {
            // The line is what is drawn now.
            let copies = stroke
                .canvas
                .as_ref()
                .map(|canvas| canvas.copies.clone())
                .unwrap_or_default();
            keep.recorder = Recorder::new(
                replay::ENGINE,
                keep.brush.hash(),
                &stroke.settings,
                Colors {
                    main: keep.colors[0],
                    sub: keep.colors[1],
                },
                copies,
            );
        }
        stroke.raw.clear();
        stroke.lifted = false;
        for sample in samples {
            stroke.send(sample);
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            if plain {
                let shown: Arc<dyn crate::composite::LiveStroke> =
                    Arc::new(crate::composite::BrushLive::new(
                        stroke.layer,
                        Arc::clone(&stroke.copies[0].live),
                    ));
                doc.compositor.set_live(&cx.world.arena, Some(shown));
            } else if !rects.is_empty() {
                doc.compositor.refresh_view(
                    &cx.world.arena,
                    &rects,
                    Some(stroke.layer),
                    visible,
                    step,
                );
            }
        }
    }

    /// Finishes `stroke` and records it.
    pub(crate) fn engine_end(&mut self, stroke: EngineStroke, cx: &mut AppletContext<'_>) {
        let EngineStroke {
            copies,
            layer,
            target,
            offset,
            lay,
            label,
            eraser,
            keep,
            transforms,
            settings,
            ..
        } = stroke;
        let lives: Vec<Arc<LiveView>> = copies.iter().map(|copy| Arc::clone(&copy.live)).collect();
        let mut finished = Vec::new();
        for copy in copies {
            let mut done = copy.worker.finish();
            if let Some(done) = done.as_mut() {
                // The layer is written next: the stroke's snapshot of it
                // goes first, so nothing is copied for it.
                done.let_go_of_layer();
            }
            finished.push(done);
        }
        let (visible, step) = self.on_screen;
        let (edits, rects) = match lay {
            Some(mut lay) => {
                // The last of what the workers laid, then what changed.
                let mut keys: Vec<_> = finished
                    .iter_mut()
                    .flatten()
                    .flat_map(sue_brush::worker::Stroke::take_dirty)
                    .collect();
                keys.sort_unstable();
                keys.dedup();
                let rects = lay.lay(&mut cx.world.arena, &keys, &lives, offset);
                (lay.edits(&cx.world.arena, &label), rects)
            }
            None => {
                let edit = finished
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(|finished| {
                        let raster = doc::raster_mut(&mut cx.world.arena, target)?;
                        finished.commit(raster, [0, 0], target, &label).ok()
                    });
                let rects = edit.as_ref().map_or_else(Vec::new, |edit| {
                    tile_rects(
                        edit.patches()
                            .iter()
                            .map(|patch| [patch.x as i32, patch.y as i32]),
                        offset,
                    )
                });
                (edit.into_iter().collect(), rects)
            }
        };
        // The stroke is in its rasters now (or lost): nothing more is
        // shown of it, and what it changed is shown from them.
        for live in &lives {
            live.end();
        }
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.set_live(&cx.world.arena, None);
            if rects.is_empty() {
                doc.compositor.accept(&cx.world.arena, layer);
            } else {
                doc.compositor
                    .refresh_view(&cx.world.arena, &rects, Some(layer), visible, step);
            }
        }
        let mut edits: Vec<_> = edits.into_iter().filter(|edit| !edit.is_empty()).collect();
        if edits.is_empty() {
            return;
        }
        if let Some(keep) = keep {
            // A vector layer keeps the stroke as a line of the engine, in
            // the same undo step as its pixels; Vector magnet joins its
            // ends to the ends near them.
            let line = engine_line(keep, &transforms, &settings);
            let edit = edits.remove(0);
            let old = crate::vector::geometry(&cx.world.arena, layer);
            let lines = crate::vector::lines(&old);
            let (lines, pixels) = self.magnet_stroke(cx, layer, line, lines, edit);
            let edit = crate::vector::keep(&mut cx.world.arena, layer, old, &lines, pixels, &label);
            cx.record(ArenaEdit(edit));
            if !eraser {
                self.recent.push(self.drawing_color());
            }
            return;
        }
        if edits.len() == 1 {
            cx.record(ArenaEdit(edits.remove(0)));
        } else {
            let mut batch = Batch::new(label);
            for edit in edits {
                batch = batch.then(edit);
            }
            cx.record(ArenaEdit(batch));
        }
        if !eraser {
            self.recent.push(self.drawing_color());
        }
    }

    /// Gives the stroke under way up: nothing of it stays.
    pub(super) fn engine_abandon(&mut self, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Engine(mut stroke)) = self.gesture.take() else {
            return;
        };
        let (visible, step) = self.on_screen;
        let rects = stroke
            .lay
            .as_mut()
            .map(|lay| lay.restore(&mut cx.world.arena, stroke.offset));
        // Let go while it still says what it covered, so that is shown
        // again from the layer.
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.set_live(&cx.world.arena, None);
            if let Some(rects) = rects.filter(|rects| !rects.is_empty()) {
                doc.compositor.refresh_view(
                    &cx.world.arena,
                    &rects,
                    Some(stroke.layer),
                    visible,
                    step,
                );
            }
        }
        for copy in stroke.copies {
            copy.worker.abandon();
        }
    }

    /// Settles a stroke under way when the studio closes: it ends where
    /// the pen last was.
    pub(crate) fn engine_settle(&mut self, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Engine(mut stroke)) = self.gesture.take() else {
            return;
        };
        if !stroke.lifted {
            let (at, time) = (stroke.last.pos, stroke.last.time);
            stroke.lift(at, time);
        }
        self.engine_end(*stroke, cx);
    }

    /// Smart Shape for a stroke of the engine: while its pen rests in one
    /// place long enough, the stroke is drawn again as the clean shape it
    /// was meant to be, which then follows the pen until it lifts.
    pub(crate) fn engine_smart(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        let (now, pointer, down) = ctx.input(|input| {
            (
                input.time,
                input.pointer.latest_pos(),
                input.pointer.primary_down(),
            )
        });
        let Some(Gesture::Engine(stroke)) = self.gesture.as_mut() else {
            return;
        };
        let (Some(pointer), true) = (pointer, down) else {
            return;
        };
        if !stroke.smart || stroke.line || stroke.snap.is_some() || stroke.lifted {
            return;
        }
        let pen = stroke.last.pos;
        if let Some(shaping) = stroke.shaping.as_mut() {
            // A shape already made follows the pen: a line's far end, or
            // the shape turned and sized around its middle (always from
            // the shape as it was made, the pen's way since carried over
            // it).
            if (pen[0] - shaping.at[0]).hypot(pen[1] - shaping.at[1]) < 0.5 {
                return;
            }
            let moved = crate::smart::dragged(&shaping.path, shaping.pivot, shaping.reference, pen);
            shaping.at = pen;
            let samples = shape_samples(stroke, &moved);
            let Some(Gesture::Engine(mut stroke)) = self.gesture.take() else {
                return;
            };
            self.engine_redraw(&mut stroke, samples, cx);
            self.gesture = Some(Gesture::Engine(stroke));
            return;
        }
        match stroke.rest {
            Some((since, at)) if at.distance(pointer) <= crate::smart::HOLD_REACH => {
                let waited = now - since;
                if waited < crate::smart::HOLD_SECONDS {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                        crate::smart::HOLD_SECONDS - waited,
                    ));
                    return;
                }
            }
            _ => {
                stroke.rest = Some((now, pointer));
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    crate::smart::HOLD_SECONDS,
                ));
                return;
            }
        }
        // Rested long enough: the shape, if the stroke makes one.
        let points: Vec<[f32; 2]> = stroke.raw.iter().map(|sample| sample.pos).collect();
        let step = (stroke.size() * 0.25).clamp(0.75, 4.0);
        let Some(path) = crate::smart::recognize(&points, step) else {
            // No shape fits (yet): the stroke carries on as drawn.
            stroke.rest = Some((now, pointer));
            return;
        };
        let reference = path
            .iter()
            .copied()
            .min_by(|a, b| {
                let from = |p: &[f32; 2]| (p[0] - pen[0]).hypot(p[1] - pen[1]);
                from(a).total_cmp(&from(b))
            })
            .unwrap_or(pen);
        let pivot = crate::smart::pivot(&path);
        // A shape ends where it is drawn to: it does not flick.
        stroke.brush.preset.input.flick = false;
        if let Some(keep) = stroke.keep.as_mut() {
            Arc::make_mut(&mut keep.brush).preset.input.flick = false;
        }
        let samples = shape_samples(stroke, &path);
        let Some(Gesture::Engine(mut stroke)) = self.gesture.take() else {
            return;
        };
        self.engine_redraw(&mut stroke, samples, cx);
        stroke.shaping = Some(Shaping {
            pivot,
            reference,
            at: pen,
            path,
        });
        self.gesture = Some(Gesture::Engine(stroke));
    }
}

/// Where `selection` (picture pixels) lets a stroke whose pixel `(0, 0)`
/// is picture pixel `offset` paint, in the stroke's pixels.
fn selection_region(selection: &crate::select::Selection, offset: [i64; 2]) -> Region {
    let [x0, y0, x1, y1] = selection.bounds().unwrap_or([0, 0, 0, 0]);
    let shift = |v: u32, by: i64| {
        (i64::from(v) - by).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    };
    let rect = PixelRect {
        x0: shift(x0, offset[0]),
        y0: shift(y0, offset[1]),
        x1: shift(x1, offset[0]),
        y1: shift(y1, offset[1]),
    };
    Region::from_cover(rect, |x, y| {
        let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
        match (u32::try_from(px), u32::try_from(py)) {
            (Ok(px), Ok(py)) => selection.at(px, py),
            _ => 0.0,
        }
    })
}

/// The line of the engine a stroke kept on a vector layer is: its samples
/// as the recorder kept them, what the pen said at each, its brush and
/// its copies.
fn engine_line(keep: Keep, transforms: &[[f32; 6]], settings: &Settings) -> crate::vector::Line {
    let stroke = keep.recorder.finish();
    let mut samples: Vec<crate::brush::Sample> = stroke
        .samples
        .iter()
        .map(|sample| crate::brush::Sample {
            at: sample.pos,
            pressure: sample.pressure.unwrap_or(1.0),
            speed: 0.0,
            tilt: 0.0,
            lean: [0.0, 0.0],
            width: 1.0,
            alpha: 1.0,
        })
        .collect();
    let pen = stroke
        .samples
        .iter()
        .map(|sample| crate::vector::PenPart {
            time: sample.time as f32,
            pressure: sample.pressure,
            tilt: sample.tilt,
            twist: sample.twist,
        })
        .collect();
    let engine = crate::vector::EngineLine {
        engine: stroke.engine,
        seed: stroke.seed,
        brush_hash: stroke.brush,
        brush: Some(keep.brush),
        device: settings.device,
        scale: settings.scale,
        curve: settings.curve,
        pen,
    };
    engine.show(&mut samples);
    crate::vector::Line {
        samples,
        tip: keep.tip,
        colors: keep.colors,
        finish: crate::brush::Finish::default(),
        copies: if transforms.len() > 1 {
            transforms.to_vec()
        } else {
            vec![SAME]
        },
        marks: Vec::new(),
        engine: Some(engine),
    }
}

/// A shape's `path` as the samples of a stroke: pressed as the stroke
/// was in its middle, over the time the stroke took.
fn shape_samples(stroke: &EngineStroke, path: &[[f32; 2]]) -> Vec<Sample> {
    let mut pressures: Vec<f32> = stroke
        .raw
        .iter()
        .filter_map(|sample| sample.pressure)
        .collect();
    pressures.sort_by(f32::total_cmp);
    let pressure = pressures.get(pressures.len() / 2).copied();
    let start = stroke
        .raw
        .first()
        .map_or(stroke.last.time, |sample| sample.time);
    let span = (stroke.last.time - start).max(0.05);
    let count = path.len().max(2) - 1;
    path.iter()
        .enumerate()
        .map(|(i, &pos)| {
            let kind = if i == 0 { Kind::Down } else { Kind::Move };
            let mut sample = Sample::new(kind, pos, start + span * i as f64 / count as f64);
            sample.pressure = pressure;
            sample
        })
        .collect()
}

/// The picture rectangles of the tiles at layer pixels `origins` (a
/// tile's top left), for a layer at `offset`, cut to the picture's
/// positive side.
fn tile_rects(origins: impl Iterator<Item = [i32; 2]>, offset: [i64; 2]) -> Vec<[u32; 4]> {
    let tile = i64::from(sue_brush::buffer::TILE);
    origins
        .filter_map(|[x, y]| {
            let x0 = i64::from(x) + offset[0];
            let y0 = i64::from(y) + offset[1];
            let clamp = |v: i64| v.clamp(0, i64::from(u32::MAX)) as u32;
            let rect = [clamp(x0), clamp(y0), clamp(x0 + tile), clamp(y0 + tile)];
            (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line along x with a wobble, lifted at its end.
    fn wobbly(count: usize) -> Vec<Sample> {
        let mut samples: Vec<Sample> = (0..count)
            .map(|i| {
                let kind = if i == 0 { Kind::Down } else { Kind::Move };
                let y = if i % 2 == 0 { 2.0 } else { -2.0 };
                Sample::new(kind, [i as f32 * 3.0, y], i as f64 / 200.0).pressing(0.5)
            })
            .collect();
        let last = *samples.last().unwrap();
        samples.push(Sample::new(Kind::Up, last.pos, last.time + 0.005));
        samples
    }

    /// A line that wobbles 2 pixels either side every 15 pixels, as a
    /// shaky hand draws it, sampled every 7.5 pixels.
    fn shaky() -> Vec<Sample> {
        let mut samples: Vec<Sample> = (0..=48)
            .map(|i| {
                let kind = if i == 0 { Kind::Down } else { Kind::Move };
                let y = match i % 4 {
                    1 => 2.0,
                    3 => -2.0,
                    _ => 0.0,
                };
                Sample::new(kind, [i as f32 * 7.5, y], f64::from(i) / 100.0).pressing(0.5)
            })
            .collect();
        let last = *samples.last().unwrap();
        samples.push(Sample::new(Kind::Up, last.pos, last.time + 0.005));
        samples
    }

    /// The most a corrected line strays from the straight middle.
    fn strays(samples: &[Sample]) -> f32 {
        samples[4..samples.len() - 4]
            .iter()
            .map(|s| s.pos[1].abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn bezier_post_correction_lays_a_shaky_line_straighter() {
        let raw = shaky();
        let how = Correct::default();
        let plain = corrected_as(&raw, 4, 10.0, how);
        let bezier = corrected_as(
            &raw,
            4,
            10.0,
            Correct {
                bezier: true,
                ..how
            },
        );
        assert_eq!(plain.len(), raw.len());
        assert_eq!(bezier.len(), raw.len(), "pressure and time stay");
        assert!(
            strays(&bezier) < strays(&plain) * 0.5,
            "{} against {}",
            strays(&bezier),
            strays(&plain)
        );
        // The ends stay where they were.
        assert_eq!(bezier[0].pos, raw[0].pos);
        assert_eq!(bezier[raw.len() - 2].pos, raw[raw.len() - 2].pos);
    }

    #[test]
    fn post_correction_by_speed_and_by_scale_change_how_far_it_smooths() {
        let raw = shaky();
        let how = Correct::default();
        let base = strays(&corrected_as(&raw, 3, 10.0, how));
        // 360 pixels in half a second: 720 a second at 100 %, so by speed
        // it smooths about as much; drawn four times as fast it smooths
        // more.
        let quick: Vec<Sample> = raw
            .iter()
            .map(|s| Sample {
                time: s.time / 4.0,
                ..*s
            })
            .collect();
        let by_speed = Correct {
            by_speed: true,
            ..how
        };
        assert!(strays(&corrected_as(&quick, 3, 10.0, by_speed)) < base);
        // By scale: at 400 % the same smoothing on the screen is a quarter
        // of the pixels, at 25 % four times as many.
        let at = |zoom: f32| {
            let by_scale = Correct {
                by_scale: true,
                zoom,
                ..how
            };
            strays(&corrected_as(&raw, 3, 10.0, by_scale))
        };
        assert!(
            at(4.0) > base && at(0.25) < base,
            "{} {base} {}",
            at(4.0),
            at(0.25)
        );
    }

    #[test]
    fn post_correction_smooths_the_middle_and_keeps_the_ends() {
        let samples = wobbly(60);
        let smooth = corrected(&samples, 10, 40.0);
        assert_eq!(smooth.len(), samples.len());
        assert_eq!(smooth[0].pos, samples[0].pos, "the start stays");
        assert_eq!(smooth[59].pos, samples[59].pos, "the end stays");
        let wobble = |s: &[Sample]| s[20..40].iter().map(|p| p.pos[1].abs()).fold(0.0, f32::max);
        assert!(
            wobble(&smooth) < wobble(&samples) * 0.5,
            "the wobble is gone"
        );
        assert_eq!(
            smooth.last().unwrap().pos,
            smooth[59].pos,
            "the lift ends there"
        );
        for (a, b) in smooth.iter().zip(&samples) {
            assert_eq!((a.time, a.pressure, a.kind), (b.time, b.pressure, b.kind));
        }
        assert_eq!(corrected(&samples, 0, 8.0), samples, "level 0 is none");
    }

    #[test]
    fn post_correction_keeps_a_corner_sharp() {
        // Right 120 px, then down 120 px.
        let mut samples = Vec::new();
        for i in 0..=40 {
            samples.push([i as f32 * 3.0, 0.0]);
        }
        for i in 1..=40 {
            samples.push([120.0, i as f32 * 3.0]);
        }
        let samples: Vec<Sample> = samples
            .iter()
            .enumerate()
            .map(|(i, &pos)| {
                let kind = if i == 0 { Kind::Down } else { Kind::Move };
                Sample::new(kind, pos, i as f64 / 200.0).pressing(0.5)
            })
            .collect();
        let smooth = corrected(&samples, 10, 8.0);
        let corner = smooth[40].pos;
        assert!(
            (corner[0] - 120.0).abs() < 0.5 && corner[1].abs() < 0.5,
            "the corner stays: {corner:?}"
        );
    }

    #[test]
    fn a_mask_draws_every_brush_as_its_round_tip() {
        let toolbox = crate::tools::Toolbox::default();
        let mut rounded = 0;
        for tool in [Tool::Pen, Tool::Brush, Tool::Airbrush, Tool::Decoration] {
            for sub in toolbox.subs(tool) {
                let brush = brush_of(sub, false, false).expect("a brush");
                let round = brush_of(sub, false, true).expect("round on a mask");
                assert_eq!(round.preset.tip, TipSource::Round, "{}", sub.name);
                assert!(round.pictures.is_empty(), "{}", sub.name);
                assert!(
                    !round.preset.build_up
                        && round.preset.mix.is_none()
                        && round.preset.texture.is_none(),
                    "{}",
                    sub.name
                );
                assert!(
                    Brush::new(round.preset.clone(), [1.0; 3]).is_plain(),
                    "{}: laid as coverage on a mask",
                    sub.name
                );
                rounded += usize::from(!Brush::new(brush.preset, [1.0; 3]).is_plain());
            }
        }
        assert!(
            rounded > 10,
            "brushes with behaviours were rounded: {rounded}"
        );
        let mut toolbox = toolbox;
        toolbox.pick(Tool::Pen);
        let eraser = brush_of(toolbox.current(), true, false).unwrap();
        assert!(eraser.preset.erase, "the transparent color erases");
    }
}
