//! The recorder: device samples in, the stroke as it is kept, and the
//! same samples handed on to the live pipeline.

use sue_model::brush::ContentHash;

use super::stroke::StoredStroke;
use crate::input::{Kind, Sample, Settings};
use crate::paint::Colors;

/// Keeps a stroke while it is drawn. Every device sample goes through
/// [`Self::take`] before the live pipeline sees it, so the live stroke
/// and its replay read the very same numbers.
#[derive(Clone, Debug)]
pub struct Recorder {
    stroke: StoredStroke,
    to_layer: [[f32; 3]; 2],
    /// The device time of the landing.
    start: Option<f64>,
}

impl Recorder {
    /// A recorder for a stroke drawn as `settings` say, with the brush
    /// whose bytes hash to `brush`, in `colors`, through `copies` (empty
    /// for none), painted by engine `engine` ([`super::ENGINE`]).
    #[must_use]
    pub fn new(
        engine: u32,
        brush: ContentHash,
        settings: &Settings,
        colors: Colors,
        copies: Vec<[f32; 6]>,
    ) -> Self {
        Self {
            stroke: StoredStroke::new(engine, brush, settings, colors, copies),
            to_layer: settings.to_layer,
            start: None,
        }
    }

    /// The settings to build the live pipeline with: the stroke's own,
    /// in layer space, since [`Self::take`] maps the samples there.
    #[must_use]
    pub fn settings(&self) -> Settings {
        self.stroke.settings()
    }

    /// Takes a device sample and returns it as kept (in layer pixels,
    /// time counted from the landing and held to the precision a vector
    /// layer keeps it in), for the live pipeline; `None` for a sample
    /// that is not part of the stroke (hovering before the landing, or
    /// anything after the lift), which the pipeline would not read
    /// either.
    pub fn take(&mut self, sample: Sample) -> Option<Sample> {
        if self.stroke.is_finished() {
            return None;
        }
        let start = match self.start {
            Some(start) => start,
            None if sample.kind == Kind::Down => {
                let start = if sample.time.is_finite() {
                    sample.time
                } else {
                    0.0
                };
                self.start = Some(start);
                start
            }
            None => return None,
        };
        let [x, y] = sample.pos;
        let m = &self.to_layer;
        let kept = Sample {
            pos: [
                m[0][0] * x + m[0][1] * y + m[0][2],
                m[1][0] * x + m[1][1] * y + m[1][2],
            ],
            // Seconds since the landing, as an f32 holds them: a vector
            // layer keeps times that way.
            time: f64::from((sample.time - start) as f32),
            ..sample
        };
        self.stroke.samples.push(kept);
        Some(kept)
    }

    /// The pen still where it last was, at device time `time`: the last
    /// sample again (a move), for spraying while the device is silent.
    /// `None` before the landing and after the lift.
    pub fn still(&mut self, time: f64) -> Option<Sample> {
        let start = self.start?;
        let last = *self.stroke.samples.last()?;
        if last.kind == Kind::Up {
            return None;
        }
        let kept = Sample {
            kind: Kind::Move,
            time: f64::from((time - start) as f32),
            ..last
        };
        if kept.time <= last.time {
            return None;
        }
        self.stroke.samples.push(kept);
        Some(kept)
    }

    /// The stroke as kept so far.
    #[must_use]
    pub fn stroke(&self) -> &StoredStroke {
        &self.stroke
    }

    /// The stroke as kept.
    #[must_use]
    pub fn finish(self) -> StoredStroke {
        self.stroke
    }
}
