//! The new brush engine on the canvas, behind the hidden setting
//! `sue_paint.brush_engine = "new"` (D2 of the 0.0.8 plan): the round pen
//! and the pencil sub tools draw through `sue_brush::worker`, so the
//! owner can draw with the engine before every tool moves onto it.
//!
//! The canvas only queues samples. Every packet of the pen's stream
//! (`sue_platform::stylus::take_pen_samples`: a WinTab packet, a Windows
//! Ink touch, a mouse move) goes to the stroke's worker with its own
//! place, time and pressure, mapped into the layer's pixels; the worker
//! lays them on the shared pool, and the compositor shows the stroke over
//! its layer until the lift merges it in as one undo step. The window's
//! release is not a point of the line: it ends the stroke where the pen
//! last was.
//!
//! Where no sample of the stream belongs to the stroke (a script driving
//! the studio with pointer events alone), the pointer's events are its
//! samples, pressing as hard as the pen pressure Sue Paint reads.
//!
//! A stroke the new engine cannot draw yet goes to the legacy engine as
//! before: a selection, a locked or masked layer, a vector layer, the
//! rulers and grid snapping, Shift's straight line, a sub tool that dabs
//! (build-up, mixing, pictures). Paper grain, Smart Shape, post
//! correction and flicks are not drawn by the new engine yet (P3-7 and
//! P4-1a).

use std::sync::Arc;

use egui::{Modifiers, Pos2, Rect};
use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::worker::{Brush, LiveView, StrokeWorker};
use sue_core::EntityId;
use sue_model::BrushPreset;
use sue_model::brush::{EngineKind, TipSource};
use sue_platform::stylus::{PenKind, PenSample, PenSource};
use sue_wm::{AppletContext, ArenaEdit};

use crate::doc;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

/// The rice key that picks the engine: `legacy` (the default) or `new`.
pub(crate) const ENGINE_KEY: &str = "sue_paint.brush_engine";

/// How many of the pen's samples wait at most for a stroke to begin.
const PENDING: usize = 4096;

/// How far from the press a stream sample may be to be the press, in
/// screen points.
const NEAR: f32 = 24.0;

/// A stroke of the new engine under way.
#[derive(Debug)]
pub(crate) struct EngineStroke {
    worker: StrokeWorker,
    live: Arc<LiveView>,
    layer: EntityId,
    /// The layer's place in the picture.
    offset: [i64; 2],
    label: String,
    eraser: bool,
    /// The stream source whose samples are the stroke's; `None` when the
    /// pointer's events are.
    source: Option<PenSource>,
    /// The last sample given to the worker.
    last: Sample,
    /// The lift has been given.
    lifted: bool,
}

/// The brush a sub tool's tip makes, when the new engine draws it: a
/// swept round tip.
fn preset_of(sub: &SubTool, eraser: bool) -> Option<BrushPreset> {
    if !matches!(sub.tool, Tool::Pen | Tool::Pencil) {
        return None;
    }
    let mut preset = BrushPreset::from_legacy_text(&sub.tip.to_text());
    let round = preset.engine == EngineKind::Stroke
        && matches!(preset.tip, TipSource::Round)
        && preset.mix.is_none()
        && !preset.build_up;
    if !round {
        return None;
    }
    preset.input.stabilization = sub.stabilization;
    preset.input.by_speed = sub.adjust_by_speed;
    preset.erase |= eraser;
    Some(preset)
}

impl SuePaint {
    /// `true` when the new brush engine draws (`sue_paint.brush_engine`).
    pub(super) fn engine_on(cx: &AppletContext<'_>) -> bool {
        cx.rice().get_str(ENGINE_KEY) == Some("new")
    }

    /// Takes this frame's pen samples from the stream and keeps those
    /// since the latest touch down, for a stroke that starts with it.
    pub(super) fn engine_take_samples(&mut self, cx: &AppletContext<'_>) {
        if !Self::engine_on(cx) {
            return;
        }
        for sample in sue_platform::stylus::take_pen_samples() {
            if sample.kind == PenKind::Down && !matches!(self.gesture, Some(Gesture::Engine(_))) {
                self.pen_stream.clear();
            }
            if self.pen_stream.len() < PENDING {
                self.pen_stream.push(sample);
            }
        }
    }

    /// Where screen point `pos` is in the active layer's pixels.
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
        let pos = self.layer_point(
            Pos2::new(sample.pos[0], sample.pos[1]),
            viewport,
            size,
            offset,
        );
        Sample {
            pos,
            time: sample.time,
            pressure: sample.pressure,
            tilt: sample.tilt,
            twist: sample.twist,
            kind,
        }
    }

    /// Starts a stroke of the new engine for `sub` at `pos`, when it is
    /// on and can draw this stroke. `true` when it did.
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
        if !Self::engine_on(cx) || modifiers.shift || self.snap_grid {
            return false;
        }
        let Some(preset) = preset_of(sub, self.transparent) else {
            return false;
        };
        let Some(doc) = self.docs.get(self.active) else {
            return false;
        };
        let Ok(info) = self.drawable(&cx.world.arena) else {
            return false;
        };
        let arena = &cx.world.arena;
        let rulers = crate::rulers::Rulers::of(arena, doc.picture);
        if doc.selection.is_some()
            || doc.mask_edit
            || info.alpha_locked
            || crate::vector::is_vector(arena, info.id)
            || (doc.show_rulers && doc.ruler_on && !rulers.is_empty())
            || doc::raster(arena, info.id).is_none_or(|raster| raster.bytes().is_none())
        {
            return false;
        }
        let zoom = doc.view.zoom;
        let offset = info.style.offset;
        let color = crate::color::unit(self.drawing_color());
        let brush = Brush::new(preset, color);
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
                // The pen pressure Sue Paint reads is through the curve
                // already.
                let at = self.layer_point(pos, viewport, size, offset);
                first.push(Sample::new(Kind::Down, at, self.clock.0).pressing(self.pen_pressure()));
                None
            }
        };
        settings.seed = self.clock.0.to_bits();
        let pool = crate::composite::share_brush_pool();
        let mut worker = StrokeWorker::start(pool, &brush, settings);
        let live = worker.live();
        let lifted = first.iter().any(|sample| sample.kind == Kind::Up);
        let last = *first
            .last()
            .unwrap_or(&Sample::new(Kind::Down, [0.0, 0.0], 0.0));
        worker.push_all(&first);
        if let Some(doc) = self.docs.get_mut(self.active) {
            let shown: Arc<dyn crate::composite::LiveStroke> =
                Arc::new(crate::composite::BrushLive::new(info.id, Arc::clone(&live)));
            doc.compositor.set_live(&cx.world.arena, Some(shown));
        }
        self.gesture = Some(Gesture::Engine(Box::new(EngineStroke {
            worker,
            live,
            layer: info.id,
            offset,
            label: crate::localize(&sub.name),
            eraser: brush.preset.erase,
            source,
            last,
            lifted,
        })));
        true
    }

    /// The pointer moved to `pos` during a stroke drawn from the
    /// pointer's events: one more sample.
    pub(super) fn engine_drag(&mut self, pos: Pos2, viewport: Rect, size: [u32; 2]) {
        let pressure = self.pen_pressure();
        let time = self.clock.0;
        let Some(Gesture::Engine(stroke)) = &self.gesture else {
            return;
        };
        if stroke.source.is_some() || stroke.lifted {
            return;
        }
        let at = self.layer_point(pos, viewport, size, stroke.offset);
        let sample = Sample::new(Kind::Move, at, time).pressing(pressure);
        if let Some(Gesture::Engine(stroke)) = &mut self.gesture {
            stroke.worker.push(sample);
            stroke.last = sample;
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
            if stroke.lifted {
                break;
            }
            stroke.worker.push(sample);
            stroke.last = sample;
            stroke.lifted = sample.kind == Kind::Up;
        }
    }

    /// Once a frame: the frame's samples to the worker, and what it laid
    /// since the last frame shown.
    pub(super) fn engine_frame(&mut self, viewport: Rect, size: [u32; 2], cx: &AppletContext<'_>) {
        self.engine_feed(viewport, size);
        let (visible, step) = self.on_screen;
        let Some(Gesture::Engine(stroke)) = &mut self.gesture else {
            return;
        };
        let dirty = stroke.worker.poll();
        let (layer, offset) = (stroke.layer, stroke.offset);
        if dirty.is_empty() {
            return;
        }
        let rects = tile_rects(dirty.iter().map(|key| key.origin()), offset);
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
            let at = match stroke.source {
                Some(_) => stroke.last.pos,
                None => self.layer_point(pos, viewport, size, stroke.offset),
            };
            let time = stroke.last.time.max(self.clock.0);
            stroke.worker.push(Sample::new(Kind::Up, at, time));
        }
        self.engine_end(*stroke, cx);
    }

    /// Finishes `stroke` and records it.
    pub(crate) fn engine_end(&mut self, stroke: EngineStroke, cx: &mut AppletContext<'_>) {
        let EngineStroke {
            worker,
            live,
            layer,
            offset,
            label,
            eraser,
            ..
        } = stroke;
        let finished = worker.finish();
        let edit = finished.as_ref().and_then(|finished| {
            let raster = doc::raster_mut(&mut cx.world.arena, layer)?;
            finished.commit(raster, [0, 0], layer, &label).ok()
        });
        // The stroke is in the layer now (or lost): nothing more is shown
        // of it, and what it changed is shown from the layer.
        live.end();
        let (visible, step) = self.on_screen;
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.set_live(&cx.world.arena, None);
            let rects = edit.as_ref().map_or_else(Vec::new, |edit| {
                tile_rects(
                    edit.patches()
                        .iter()
                        .map(|patch| [patch.x as i32, patch.y as i32]),
                    offset,
                )
            });
            if rects.is_empty() {
                doc.compositor.accept(&cx.world.arena, layer);
            } else {
                doc.compositor
                    .refresh_view(&cx.world.arena, &rects, Some(layer), visible, step);
            }
        }
        if let Some(edit) = edit.filter(|edit| !edit.is_empty()) {
            cx.record(ArenaEdit(edit));
            if !eraser {
                self.recent.push(self.drawing_color());
            }
        }
    }

    /// Gives the stroke under way up: nothing of it stays.
    pub(super) fn engine_abandon(&mut self, cx: &AppletContext<'_>) {
        let Some(Gesture::Engine(stroke)) = self.gesture.take() else {
            return;
        };
        // Let go while it still says what it covered, so that is shown
        // again from the layer.
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.set_live(&cx.world.arena, None);
        }
        stroke.worker.abandon();
    }

    /// Settles a stroke under way when the studio closes: it ends where
    /// the pen last was.
    pub(crate) fn engine_settle(&mut self, cx: &mut AppletContext<'_>) {
        let Some(Gesture::Engine(mut stroke)) = self.gesture.take() else {
            return;
        };
        if !stroke.lifted {
            let up = Sample::new(Kind::Up, stroke.last.pos, stroke.last.time);
            stroke.worker.push(up);
        }
        self.engine_end(*stroke, cx);
    }
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
