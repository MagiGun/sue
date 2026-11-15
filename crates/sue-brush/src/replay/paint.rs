//! Painting a stored stroke again.

use sue_core::RasterTile2D;
use sue_model::BrushPreset;

use super::ENGINE;
use super::stroke::StoredStroke;
use crate::buffer::MergeError;
use crate::input::{Kind, Pipeline, Sample};
use crate::math::atan2;
use crate::paint::finish::Field;
use crate::paint::lay::lay_field;
use crate::paint::wet::{Backdrop, RasterBackdrop};
use crate::paint::{Behaviours, PaintStroke, Resources};
use crate::raster::PixelRect;

/// Why a stored stroke cannot be painted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// It was painted by another engine version than this build's
    /// [`ENGINE`]; that version's code paints it.
    Engine(u32),
    /// The raster holds no color.
    Merge(MergeError),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(version) => write!(
                f,
                "the stroke was painted by brush engine {version}, this build paints with {ENGINE}"
            ),
            Self::Merge(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<MergeError> for ReplayError {
    fn from(error: MergeError) -> Self {
        Self::Merge(error)
    }
}

/// `sample` through `copy`: its place, and the way the pen leans and is
/// turned, turned and mirrored with it.
fn through(sample: &Sample, copy: &[f32; 6]) -> Sample {
    let [a, b, c, d, e, f] = *copy;
    let [x, y] = sample.pos;
    let tilt = sample
        .tilt
        .map(|[tx, ty]| [a * tx + b * ty, c * tx + d * ty]);
    // The copy's own turn, in degrees; a mirror turns the pen the other
    // way round.
    let turn = atan2(c, a).to_degrees();
    let mirrored = a * d - b * c < 0.0;
    let twist = sample
        .twist
        .map(|t| if mirrored { turn - t } else { turn + t });
    Sample {
        pos: [a * x + b * y + e, c * x + d * y + f],
        tilt,
        twist,
        ..*sample
    }
}

/// Drives one copy of `stroke` (`copy`, or as drawn) through the input
/// pipeline and a paint stroke of `preset`, sample by sample, as the
/// live worker does (see the module), and returns the finished stroke,
/// not yet laid. A mixing stroke reads the picture from `backdrop`.
///
/// # Errors
/// [`ReplayError::Engine`] for a stroke of another engine version.
pub fn painted<'a>(
    stroke: &StoredStroke,
    preset: &BrushPreset,
    resources: Resources<'a>,
    copy: Option<&[f32; 6]>,
    backdrop: Option<&dyn Backdrop>,
) -> Result<PaintStroke<'a>, ReplayError> {
    if stroke.engine != ENGINE {
        return Err(ReplayError::Engine(stroke.engine));
    }
    let registry = crate::paint::registry();
    let mut pipeline = Pipeline::new(preset, &registry, stroke.settings());
    let mut paint = PaintStroke::new(
        preset,
        &Behaviours::from_preset(preset),
        resources,
        stroke.colors,
        stroke.seed,
        pipeline.extra_keys(),
    );
    let mut done = 0;
    for sample in &stroke.samples {
        let sample = copy.map_or(*sample, |copy| through(sample, copy));
        pipeline.push(sample);
        let committed = pipeline.committed();
        if committed.len() > done {
            paint.commit(&committed[done..], backdrop);
            done = committed.len();
        }
        if sample.kind != Kind::Up {
            paint.hold(sample.time, backdrop);
        }
    }
    paint.finish(backdrop);
    Ok(paint)
}

/// `fields` laid one over another into one: each pixel takes the most
/// coverage any of them has there, and with it that one's color, so
/// copies that overlap never paint twice.
fn strongest(fields: &[Field]) -> Option<Field> {
    let first = fields.first()?;
    let rect = fields.iter().skip(1).fold(first.rect, |r, f| PixelRect {
        x0: r.x0.min(f.rect.x0),
        y0: r.y0.min(f.rect.y0),
        x1: r.x1.max(f.rect.x1),
        y1: r.y1.max(f.rect.y1),
    });
    let mut out = Field::empty(rect);
    let colored = fields.iter().all(|f| f.color.is_some());
    let mut colors = colored.then(|| vec![[0.0_f32; 3]; out.alpha.len()]);
    let width = (rect.x1 - rect.x0) as usize;
    for field in fields {
        let w = (field.rect.x1 - field.rect.x0) as usize;
        for (k, (&alpha, &dark)) in field.alpha.iter().zip(&field.dark).enumerate() {
            let (x, y) = (
                field.rect.x0 + (k % w) as i32,
                field.rect.y0 + (k / w) as i32,
            );
            let i = (y - rect.y0) as usize * width + (x - rect.x0) as usize;
            if alpha > out.alpha[i] {
                out.alpha[i] = alpha;
                if let (Some(colors), Some(theirs)) = (&mut colors, &field.color) {
                    colors[i] = theirs[k];
                }
            }
            out.dark[i] = out.dark[i].max(dark);
        }
    }
    out.color = colors;
    Some(out)
}

/// Paints `stroke`, drawn with `preset` (the brush its hash names) and
/// reading `resources`, onto `raster`, whose pixel `(0, 0)` is layer
/// pixel `origin`; with `keep_alpha` the layer's transparent pixels stay
/// so. Returns the rectangles it may have changed.
///
/// The copies of a symmetry ruler are one stroke: where they overlap
/// they lay paint once, as the strongest of them. A mixing stroke's
/// copies mix one after another, each reading what the one before laid.
///
/// # Errors
/// [`ReplayError::Engine`] for a stroke of another engine version,
/// [`ReplayError::Merge`] for a raster with no color.
pub fn paint(
    stroke: &StoredStroke,
    preset: &BrushPreset,
    resources: Resources<'_>,
    raster: &mut RasterTile2D,
    origin: [i32; 2],
    keep_alpha: bool,
) -> Result<Vec<PixelRect>, ReplayError> {
    let copies: Vec<Option<&[f32; 6]>> = if stroke.copies.is_empty() {
        vec![None]
    } else {
        stroke.copies.iter().map(Some).collect()
    };
    let wet =
        preset.mix.is_some() || matches!(preset.engine, sue_model::brush::EngineKind::Blend(_));
    if wet {
        // Each copy reads the picture as the copies before it left it.
        let mut changed = Vec::new();
        for copy in copies {
            let snapshot = raster.clone();
            let backdrop = RasterBackdrop {
                raster: &snapshot,
                origin,
            };
            let done = painted(stroke, preset, resources, copy, Some(&backdrop))?;
            changed.extend(done.merge(raster, origin, keep_alpha)?);
        }
        return Ok(changed);
    }
    let strokes = copies
        .into_iter()
        .map(|copy| painted(stroke, preset, resources, copy, None))
        .collect::<Result<Vec<_>, _>>()?;
    if let [only] = strokes.as_slice() {
        return Ok(only.merge(raster, origin, keep_alpha)?);
    }
    let fields: Vec<Field> = strokes
        .iter()
        .filter_map(|s| s.bounds().and_then(|rect| s.field(rect, false)))
        .collect();
    let Some(field) = strongest(&fields) else {
        return Ok(Vec::new());
    };
    let lay = strokes[0].lay();
    Ok(lay_field(&field, raster, origin, lay, keep_alpha)?
        .into_iter()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mirror_turns_the_pen_the_other_way() {
        let sample = Sample {
            tilt: Some([0.5, 0.0]),
            twist: Some(30.0),
            ..Sample::new(Kind::Move, [10.0, 20.0], 0.5)
        };
        // Mirrored across the vertical line x = 50.
        let mirror = [-1.0, 0.0, 0.0, 1.0, 100.0, 0.0];
        let out = through(&sample, &mirror);
        assert_eq!(out.pos, [90.0, 20.0]);
        assert_eq!(out.tilt, Some([-0.5, 0.0]));
        assert!((out.twist.unwrap() - 150.0).abs() < 1.0e-3);
        // Turned a quarter: the twist turns with it.
        let quarter = [0.0, -1.0, 1.0, 0.0, 0.0, 0.0];
        let out = through(&sample, &quarter);
        assert_eq!(out.pos, [-20.0, 10.0]);
        assert!((out.twist.unwrap() - 120.0).abs() < 1.0e-3);
        assert_eq!(out.time, sample.time);
    }

    #[test]
    fn overlapping_fields_take_the_strongest_pixel() {
        let rect = |x0, x1| PixelRect {
            x0,
            y0: 0,
            x1,
            y1: 1,
        };
        let mut a = Field::empty(rect(0, 2));
        a.alpha = vec![0.5, 0.2];
        let mut b = Field::empty(rect(1, 3));
        b.alpha = vec![0.6, 0.9];
        let out = strongest(&[a, b]).unwrap();
        assert_eq!(out.rect, rect(0, 3));
        assert_eq!(out.alpha, vec![0.5, 0.6, 0.9]);
        assert!(strongest(&[]).is_none());
    }
}
