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
use crate::buffer::{Combine, MergeError, StrokeBuffer};
use crate::input::StrokePoint;
use crate::raster::{PixelRect, TipImage};
use crate::texture::{Paper, Texture};

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
/// names by hash) and the anti-overflow region.
#[derive(Clone, Copy, Debug, Default)]
pub struct Resources<'a> {
    /// The tip's pictures, in the preset's order.
    pub pictures: &'a [TipImage],
    /// The dual brush's pictures, in its order.
    pub dual_pictures: &'a [TipImage],
    /// The paper, when the brush is textured.
    pub paper: Option<&'a Paper>,
    /// Where anti-overflow keeps the paint.
    pub overflow: Option<&'a Region>,
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
pub struct PaintStroke<'a> {
    behaviours: Behaviours,
    lay: Lay,
    target: Target,
    planner: Planner,
    dual: Option<Dual>,
    tips: Tips<'a>,
    dual_tips: Tips<'a>,
    texture: Option<Texture<'a>>,
    overflow: Option<&'a Region>,
    /// Committed points not yet planned, kept while corners are rounded
    /// (sharp angles off), with the points before them they read.
    waiting: Vec<StrokePoint>,
    /// How many of `waiting` at its start are context only.
    context: usize,
    scratch: Vec<Planned>,
}

impl<'a> PaintStroke<'a> {
    /// A stroke of `preset` with `behaviours`, in `colors`, with chance
    /// from `seed`, whose points carry their extra parameters in the
    /// order of `extra_keys` ([`crate::input::Pipeline::extra_keys`]).
    #[must_use]
    pub fn new(
        preset: &BrushPreset,
        behaviours: &Behaviours,
        resources: Resources<'a>,
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
        let lay = if preset.erase {
            Lay::Erase { opacity: 1.0 }
        } else if behaviours.behind {
            Lay::Behind {
                color: jitter.main(),
                opacity: 1.0,
            }
        } else {
            Lay::Paint {
                color: jitter.main(),
                opacity: 1.0,
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
            (Some(choice), Some(paper)) => Some(Texture::new(paper, choice.settings)),
            _ => None,
        };
        Self {
            behaviours: behaviours.clone(),
            lay,
            target,
            planner,
            dual,
            tips: Tips {
                pictures: resources.pictures,
                edge,
            },
            dual_tips: Tips {
                pictures: resources.dual_pictures,
                edge,
            },
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

    /// The wet canvas, for a mixing stroke: what the live view shows in
    /// place of the layer's tiles it touched.
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
            stamp_coverage(&mut dual.buffer.committed(), &marks, &self.dual_tips, None);
        }
        self.scratch = out;
    }

    /// Stamps planned dabs into the committed part or the tail.
    fn stamp(&mut self, tail: bool, dabs: &[Planned], backdrop: Option<&dyn Backdrop>) {
        let texture = self.texture.as_ref().filter(|t| t.settings().per_dab);
        match &mut self.target {
            Target::Coverage(buffer) => {
                let mut painter = if tail {
                    buffer.tail()
                } else {
                    buffer.committed()
                };
                stamp_coverage(&mut painter, dabs, &self.tips, texture);
            }
            Target::Color(buffer) => {
                if tail {
                    buffer.clear_tail();
                }
                buffer.stamp(tail, dabs, &self.tips, texture);
            }
            Target::Wet(canvas) => {
                // A wet stroke reads what it laid a moment before, so
                // only final points paint it.
                if let (false, Some(backdrop)) = (tail, backdrop) {
                    canvas.stamp(backdrop, dabs, &self.tips, texture);
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
            stamp_coverage(&mut dual.buffer.tail(), &marks, &self.dual_tips, None);
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
    /// waits and folds the tail in.
    pub fn finish(&mut self, backdrop: Option<&dyn Backdrop>) {
        let end = self.waiting.len();
        self.flush(end, backdrop);
        self.waiting.clear();
        self.context = 0;
        match &mut self.target {
            Target::Coverage(buffer) => buffer.settle(),
            Target::Color(buffer) => buffer.settle(),
            Target::Wet(_) => {}
        }
        if let Some(dual) = &mut self.dual {
            dual.buffer.settle();
        }
    }

    /// The shaping of the whole stroke; `live` while the pen is down.
    fn finishing(&self, live: bool) -> Finish<'_> {
        Finish {
            dual: self.dual.as_ref().map(|d| (&d.buffer, d.settings.mode)),
            texture: self.texture.as_ref().filter(|t| !t.settings().per_dab),
            overflow: self.overflow,
            watercolor: self
                .behaviours
                .watercolor
                .filter(|edge| !(live && edge.after_stroke)),
            tone: self.behaviours.tone,
        }
    }

    /// The pixels the stroke spans so far, or `None` when it is empty.
    #[must_use]
    pub fn bounds(&self) -> Option<PixelRect> {
        let apron = self.finishing(false).apron();
        let rect = match &self.target {
            Target::Coverage(buffer) => buffer.bounds(),
            Target::Color(buffer) => buffer.bounds(),
            Target::Wet(_) => None,
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
            return canvas.merge(raster, origin);
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
        super::lay::erase_layers(&field, targets, 1.0)
    }
}
