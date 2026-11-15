//! One stroke with every paint behaviour, from stroke points to the
//! layer: what a studio's brush adapter (and the worker, P3-6) drives.
//!
//! A [`PaintStroke`] picks where the stroke collects its paint: as
//! coverage ([`StrokeBuffer`]) for a brush of one color, with a color
//! per pixel ([`ColorBuffer`]) when its dabs differ in color, or on a
//! [`WetCanvas`] when it mixes with the picture. It plans the dabs
//! ([`Planner`]), stamps them, keeps the dual brush's mask beside them,
//! and on lift shapes the whole stroke ([`Finish`]) and lays it
//! ([`lay_field`]) in the brush's blend mode, behind the layer, or as
//! an eraser on one layer or on many.

use std::sync::Arc;

use sue_core::RasterTile2D;
use sue_model::BrushPreset;
use sue_model::brush::{EngineKind, Spacing};

use super::color::Jitter;
use super::color_buffer::ColorBuffer;
use super::corners::{WINDOW, rounded};
use super::finish::{Field, Finish};
use super::lay::{EraseTarget, Lay, lay_field};
use super::overflow::Region;
use super::plan::{PlanSettings, Planned, Planner, edge_of};
use super::settings::{Behaviours, DualBrush, TipOrder};
use super::stamp::{Tips, stamp_coverage};
use super::wet::{Backdrop, Wet, WetCanvas, blender};
use crate::buffer::{Combine, MergeError, StrokeBuffer, TILE, TileKey};
use crate::input::StrokePoint;
use crate::raster::{PixelRect, TipImage};
use crate::texture::{Paper, Texture, TextureSettings};

/// The colors a stroke paints with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    /// The main color, straight sRGB 0 to 1.
    pub main: [f32; 3],
    /// The sub color (color jitter pulls toward it).
    pub sub: [f32; 3],
}

/// What the stroke reads besides its points: the pictures its tips and
/// its paper come from (decoded by whoever holds the files the preset
/// names by hash) and the region the paint is kept in (anti-overflow,
/// a selection). Shared, so a stroke can own them and move to a worker
/// thread without copying a picture (Rule 5).
#[derive(Clone, Debug, Default)]
pub struct Resources {
    /// The tip's pictures, in the preset's order.
    pub pictures: Arc<[TipImage]>,
    /// The dual brush's pictures, in its order.
    pub dual_pictures: Arc<[TipImage]>,
    /// The paper, when the brush is textured.
    pub paper: Option<Arc<Paper>>,
    /// Where the paint may land: anti-overflow's region, a selection, or
    /// both multiplied.
    pub overflow: Option<Arc<Region>>,
}

/// Where a stroke collects its paint.
#[derive(Clone, Debug)]
enum Target {
    Coverage(StrokeBuffer),
    Color(ColorBuffer),
    Wet(WetCanvas),
}

/// The dual brush's mask and its own plan.
#[derive(Clone, Debug)]
struct Dual {
    settings: DualBrush,
    buffer: StrokeBuffer,
    planner: Planner,
}

/// One stroke with every paint behaviour.
#[derive(Clone, Debug)]
pub struct PaintStroke {
    behaviours: Behaviours,
    lay: Lay,
    target: Target,
    planner: Planner,
    dual: Option<Dual>,
    pictures: Arc<[TipImage]>,
    dual_pictures: Arc<[TipImage]>,
    /// The width of a hard round rim's antialiasing.
    edge: f32,
    texture: Option<(Arc<Paper>, TextureSettings)>,
    overflow: Option<Arc<Region>>,
    /// Committed points not yet planned, kept while corners are rounded
    /// (sharp angles off), with the points before them they read.
    waiting: Vec<StrokePoint>,
    /// How many of `waiting` at its start are context only.
    context: usize,
    scratch: Vec<Planned>,
}

impl PaintStroke {
    /// A stroke of `preset` with `behaviours`, in `colors`, with chance
    /// from `seed`, whose points carry their extra parameters in the
    /// order of `extra_keys` ([`crate::input::Pipeline::extra_keys`]).
    #[must_use]
    pub fn new(
        preset: &BrushPreset,
        behaviours: &Behaviours,
        resources: Resources,
        colors: Colors,
        seed: u64,
        extra_keys: &[String],
    ) -> Self {
        let jitter = Jitter::new(preset.color, seed, colors.main, colors.sub);
        let combine = if preset.build_up {
            Combine::BuildUp
        } else {
            Combine::Max
        };
        let wet = match &preset.engine {
            EngineKind::Blend(name) => blender(name),
            _ => None,
        }
        .or_else(|| preset.mix.map(|mixing| Wet::new(mixing, 1.0)));
        let target = match wet {
            Some(wet) => Target::Wet(WetCanvas::new(wet, jitter.main())),
            None if jitter.varies() => {
                Target::Color(ColorBuffer::new(combine, behaviours.tip_blend))
            }
            None => Target::Coverage(StrokeBuffer::new(combine)),
        };
        // The stroke's own opacity, before the pen drives it.
        let opacity = super::settings::registry()
            .value(preset, sue_model::brush::param::OPACITY)
            .map_or(1.0, |o| {
                if o.is_finite() {
                    o.clamp(0.0, 1.0)
                } else {
                    1.0
                }
            });
        let lay = if preset.erase {
            Lay::Erase { opacity }
        } else if behaviours.behind {
            Lay::Behind {
                color: jitter.main(),
                opacity,
            }
        } else {
            Lay::Paint {
                color: jitter.main(),
                opacity,
                mode: preset.blend,
            }
        };
        let settings = PlanSettings {
            spacing: preset.spacing,
            build_up: preset.build_up,
            tips: resources.pictures.len(),
            order: behaviours.tip_order,
            flip: behaviours.flip,
            gap_density: behaviours.gap_density,
            spray: behaviours.spray,
            continuous: behaviours.continuous,
            paint_density: behaviours.paint_density,
            // A wet stroke lays its dabs on the picture one by one, so
            // each carries the opacity itself.
            opacity: if wet.is_some() { 1.0 } else { opacity },
        };
        let planner = Planner::new(settings, seed, jitter, extra_keys);
        let dual = behaviours.dual.clone().map(|settings| Dual {
            planner: Planner::new(
                PlanSettings {
                    spacing: Spacing::Ratio,
                    tips: resources.dual_pictures.len(),
                    order: TipOrder::Random,
                    ..PlanSettings::default()
                },
                seed ^ 0xD0A1,
                jitter,
                &[],
            ),
            settings,
            buffer: StrokeBuffer::new(Combine::Max),
        });
        let edge = edge_of(preset.antialias);
        let texture = match (&behaviours.texture, resources.paper) {
            (Some(choice), Some(paper)) => Some((paper, choice.settings)),
            _ => None,
        };
        Self {
            behaviours: behaviours.clone(),
            lay,
            target,
            planner,
            dual,
            pictures: resources.pictures,
            dual_pictures: resources.dual_pictures,
            edge,
            texture,
            overflow: resources.overflow,
            waiting: Vec::new(),
            context: 0,
            scratch: Vec::new(),
        }
    }

    /// `true` when the stroke mixes with the picture (a wet canvas).
    #[must_use]
    pub fn is_wet(&self) -> bool {
        matches!(self.target, Target::Wet(_))
    }

    /// How the stroke meets the layer.
    #[must_use]
    pub const fn lay(&self) -> Lay {
        self.lay
    }

    /// The wet canvas, for a mixing stroke, as the dabs left it (before
    /// anti-overflow and lock transparent pixels; the live view reads
    /// [`Self::wet_pixel`]).
    #[must_use]
    pub fn wet(&self) -> Option<&WetCanvas> {
        match &self.target {
            Target::Wet(canvas) => Some(canvas),
            _ => None,
        }
    }

    /// The point the dual brush's plan reads for `p`.
    fn dual_point(dual: &DualBrush, p: &StrokePoint) -> StrokePoint {
        let size = if dual.linked {
            p.size * dual.size
        } else if p.size > 0.0 {
            dual.size
        } else {
            0.0
        };
        StrokePoint {
            size,
            spacing: dual.spacing,
            scatter: dual.scatter,
            hardness: dual.hardness,
            opacity: 1.0,
            density: 1.0,
            flow: 1.0,
            aspect: 1.0,
            angle: 0.0,
            extra: Vec::new(),
            ..p.clone()
        }
    }

    /// Plans and stamps `points` (final ones) into the committed part.
    fn lay_down(&mut self, points: &[StrokePoint], backdrop: Option<&dyn Backdrop>) {
        let mut out = std::mem::take(&mut self.scratch);
        out.clear();
        self.planner.push_all(points, &mut out);
        self.stamp(false, &out, backdrop);
        if let Some(dual) = &mut self.dual {
            let mut marks = Vec::new();
            for p in points {
                dual.planner
                    .push(&Self::dual_point(&dual.settings, p), &mut marks);
            }
            stamp_coverage(&mut dual.buffer.committed(), &marks, &Tips { pictures: &self.dual_pictures, edge: self.edge }, None);
        }
        self.scratch = out;
    }

    /// Stamps planned dabs into the committed part or the tail.
    fn stamp(&mut self, tail: bool, dabs: &[Planned], backdrop: Option<&dyn Backdrop>) {
        let texture = self
            .texture
            .as_ref()
            .filter(|(_, settings)| settings.per_dab)
            .map(|(paper, settings)| Texture::new(paper, *settings));
        let texture = texture.as_ref();
        let tips = Tips {
            pictures: &self.pictures,
            edge: self.edge,
        };
        match &mut self.target {
            Target::Coverage(buffer) => {
                let mut painter = if tail {
                    buffer.tail()
                } else {
                    buffer.committed()
                };
                stamp_coverage(&mut painter, dabs, &tips, texture);
            }
            Target::Color(buffer) => {
                if tail {
                    buffer.clear_tail();
                }
                buffer.stamp(tail, dabs, &tips, texture);
            }
            Target::Wet(canvas) => {
                // A wet stroke reads what it laid a moment before, so
                // only final points paint it.
                if let (false, Some(backdrop)) = (tail, backdrop) {
                    canvas.stamp(backdrop, dabs, &tips, texture);
                }
            }
        }
    }

    /// The window of arc length corners are rounded over, at most.
    fn window(points: &[StrokePoint]) -> f32 {
        points
            .iter()
            .map(|p| p.size * WINDOW)
            .fold(1.0_f32, f32::max)
    }

    /// Lays the stroke's newly committed points (those the input
    /// pipeline committed since the last call, in order). A wet stroke
    /// reads the picture from `backdrop`, a snapshot of the layer.
    pub fn commit(&mut self, points: &[StrokePoint], backdrop: Option<&dyn Backdrop>) {
        if self.behaviours.sharp_angles {
            self.lay_down(points, backdrop);
            return;
        }
        // Sharp angles off: a point is final once the line has gone a
        // window past it, so its corner can be rounded.
        self.waiting.extend_from_slice(points);
        let Some(last) = self.waiting.last().map(|p| p.arc) else {
            return;
        };
        let window = Self::window(&self.waiting);
        let ready = self.waiting[self.context..]
            .iter()
            .take_while(|p| p.arc + window <= last)
            .count();
        self.flush(self.context + ready, backdrop);
    }

    /// Rounds and lays `waiting` up to `end`, keeping a window of them
    /// as context for what comes next.
    fn flush(&mut self, end: usize, backdrop: Option<&dyn Backdrop>) {
        if end <= self.context {
            return;
        }
        let points: Vec<StrokePoint> = (self.context..end)
            .map(|i| StrokePoint {
                pos: rounded(&self.waiting, i),
                ..self.waiting[i].clone()
            })
            .collect();
        self.lay_down(&points, backdrop);
        // Keep what the next points' windows read.
        let window = Self::window(&self.waiting);
        let keep_from = self.waiting[..end]
            .iter()
            .position(|p| p.arc + 2.0 * window > self.waiting[end - 1].arc)
            .unwrap_or(end);
        self.waiting.drain(..keep_from);
        self.context = end - keep_from;
    }

    /// Draws the provisional tail (the input pipeline's tail, and any
    /// committed points still waiting for their corners), replacing the
    /// one before. A wet stroke shows no tail.
    pub fn tail(&mut self, tail: &[StrokePoint]) {
        if self.is_wet() {
            return;
        }
        let mut planner = self.planner.clone();
        let mut points: Vec<StrokePoint> = Vec::new();
        if self.behaviours.sharp_angles {
            points.extend_from_slice(tail);
        } else {
            let mut all = self.waiting.clone();
            all.extend_from_slice(tail);
            for i in self.context..all.len() {
                points.push(StrokePoint {
                    pos: rounded(&all, i),
                    ..all[i].clone()
                });
            }
        }
        let mut out = Vec::new();
        planner.push_all(&points, &mut out);
        self.stamp(true, &out, None);
        if let Some(dual) = &mut self.dual {
            let mut planner = dual.planner.clone();
            let mut marks = Vec::new();
            for p in &points {
                planner.push(&Self::dual_point(&dual.settings, p), &mut marks);
            }
            stamp_coverage(&mut dual.buffer.tail(), &marks, &Tips { pictures: &self.dual_pictures, edge: self.edge }, None);
        }
    }

    /// Continuous spraying: the pen held still up to device time `now`.
    pub fn hold(&mut self, now: f64, backdrop: Option<&dyn Backdrop>) {
        let mut out = Vec::new();
        self.planner.hold(now, &mut out);
        if !out.is_empty() {
            self.stamp(false, &out, backdrop);
        }
    }

    /// The pen has lifted and every point is committed: lays what still
    /// waits and drops the tail. After the lift every point is
    /// committed, so a tail drawn before it is out of date: kept, it
    /// would leave the provisional end (and, building up, paint twice)
    /// where the final one lies.
    pub fn finish(&mut self, backdrop: Option<&dyn Backdrop>) {
        let end = self.waiting.len();
        self.flush(end, backdrop);
        self.waiting.clear();
        self.context = 0;
        match &mut self.target {
            Target::Coverage(buffer) => buffer.clear_tail(),
            Target::Color(buffer) => buffer.clear_tail(),
            Target::Wet(_) => {}
        }
        if let Some(dual) = &mut self.dual {
            dual.buffer.clear_tail();
        }
    }

    /// The shaping of the whole stroke; `live` while the pen is down.
    fn finishing(&self, live: bool) -> Finish<'_> {
        Finish {
            dual: self.dual.as_ref().map(|d| (&d.buffer, d.settings.mode)),
            texture: self
                .texture
                .as_ref()
                .filter(|(_, settings)| !settings.per_dab)
                .map(|(paper, settings)| Texture::new(paper, *settings)),
            overflow: self.overflow.as_deref(),
            watercolor: self
                .behaviours
                .watercolor
                .filter(|edge| !(live && edge.after_stroke)),
            tone: self.behaviours.tone,
        }
    }

    /// For a mixing stroke, layer pixel `(x, y)` as the live view shows
    /// it: what the merge will lay there, anti-overflow and lock
    /// transparent pixels (`keep_alpha`) included. `None` for a stroke
    /// that is not wet, which shows [`Self::field`] instead.
    #[must_use]
    pub fn wet_pixel(
        &self,
        backdrop: &dyn Backdrop,
        x: i32,
        y: i32,
        keep_alpha: bool,
    ) -> Option<[f32; 4]> {
        match &self.target {
            Target::Wet(canvas) => Some(canvas.laid(backdrop, x, y, self.overflow.as_deref(), keep_alpha)),
            _ => None,
        }
    }

    /// The tiles whose laid pixels may have changed since the last call,
    /// in order: those the stroke's dabs (and its dual brush's) touched,
    /// and around them as far as the shaping of the whole stroke reads
    /// (a watercolor edge).
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        let mut keys: Vec<TileKey> = match &mut self.target {
            Target::Coverage(buffer) => buffer.take_dirty(),
            Target::Color(buffer) => buffer.take_dirty(),
            Target::Wet(canvas) => canvas.take_dirty(),
        };
        if let Some(dual) = &mut self.dual {
            keys.extend(dual.buffer.take_dirty());
        }
        let apron = self.finishing(false).apron();
        if apron > 0 && !keys.is_empty() {
            let reach = (apron + TILE - 1) / TILE;
            let mut grown = Vec::with_capacity(keys.len() * 9);
            for key in &keys {
                let [x, y] = key.origin();
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        grown.push(TileKey::of_pixel(x + dx * TILE, y + dy * TILE));
                    }
                }
            }
            keys = grown;
        }
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// `true` where a mixing stroke changed layer pixel `(x, y)`; for a
    /// stroke that is not wet, `false`.
    #[must_use]
    pub fn wet_touched(&self, x: i32, y: i32) -> bool {
        match &self.target {
            Target::Wet(canvas) => canvas.touched(x, y),
            _ => false,
        }
    }

    /// The pixels the stroke spans so far, or `None` when it is empty (a
    /// mixing stroke: the tiles it changed).
    #[must_use]
    pub fn bounds(&self) -> Option<PixelRect> {
        let apron = self.finishing(false).apron();
        let rect = match &self.target {
            Target::Coverage(buffer) => buffer.bounds(),
            Target::Color(buffer) => buffer.bounds(),
            Target::Wet(canvas) => canvas.keys().fold(None, |rect: Option<PixelRect>, key| {
                let [x, y] = key.origin();
                let tile = PixelRect {
                    x0: x,
                    y0: y,
                    x1: x + TILE,
                    y1: y + TILE,
                };
                Some(rect.map_or(tile, |r| PixelRect {
                    x0: r.x0.min(tile.x0),
                    y0: r.y0.min(tile.y0),
                    x1: r.x1.max(tile.x1),
                    y1: r.y1.max(tile.y1),
                }))
            }),
        }?;
        Some(PixelRect {
            x0: rect.x0 - apron,
            y0: rect.y0 - apron,
            x1: rect.x1 + apron,
            y1: rect.y1 + apron,
        })
    }

    /// The stroke over `rect` as it will be laid; `live` while the pen
    /// is down (an edge made "after the stroke" is left out). `None` for
    /// a wet stroke, which shows its canvas instead.
    #[must_use]
    pub fn field(&self, rect: PixelRect, live: bool) -> Option<Field> {
        let finish = self.finishing(live);
        match &self.target {
            Target::Coverage(buffer) => Some(finish.coverage(buffer, rect)),
            Target::Color(buffer) => Some(finish.color(buffer, rect)),
            Target::Wet(_) => None,
        }
    }

    /// Lays the finished stroke on `raster` (whose pixel `(0, 0)` is
    /// layer pixel `origin`); with `keep_alpha` the layer's transparent
    /// pixels stay so. Returns the rectangles it may have changed.
    ///
    /// # Errors
    /// [`MergeError::Format`] for a raster with no color.
    pub fn merge(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
        keep_alpha: bool,
    ) -> Result<Vec<PixelRect>, MergeError> {
        if let Target::Wet(canvas) = &self.target {
            return canvas.merge(raster, origin, self.overflow.as_deref(), keep_alpha);
        }
        let Some(rect) = self.bounds() else {
            return Ok(Vec::new());
        };
        let field = self
            .field(rect, false)
            .unwrap_or_else(|| Field::empty(rect));
        Ok(lay_field(&field, raster, origin, self.lay, keep_alpha)?
            .into_iter()
            .collect())
    }

    /// The finished stroke as an eraser on every layer of `targets` (the
    /// "Multiple layers" eraser), all or none.
    ///
    /// # Errors
    /// [`MergeError::Format`] when a layer holds no color.
    pub fn erase_layers(
        &self,
        targets: &mut [EraseTarget<'_>],
    ) -> Result<Vec<Option<PixelRect>>, MergeError> {
        let Some(rect) = self.bounds() else {
            return Ok(targets.iter().map(|_| None).collect());
        };
        let field = self
            .field(rect, false)
            .unwrap_or_else(|| Field::empty(rect));
        let opacity = match self.lay {
            Lay::Erase { opacity } | Lay::Paint { opacity, .. } | Lay::Behind { opacity, .. } => {
                opacity
            }
        };
        super::lay::erase_layers(&field, targets, opacity)
    }
}
