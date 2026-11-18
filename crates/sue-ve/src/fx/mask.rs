//! Masks: shapes that cut a clip's picture, as After Effects' and
//! Premiere Pro's masks do.
//!
//! A clip's masks are a list in its properties ([`super::key::MASKS`]).
//! Each is a rectangle, an ellipse or a Bezier path, in the clip's own
//! picture (0 to 1 across and down), with a feather (a soft edge, in
//! source pixels), an expansion (grown, or shrunk below 0), an opacity, a
//! mode (add, subtract, intersect) and an inversion. Feather, expansion
//! and opacity are keyframable (channel [`channel`]). Masks combine top
//! to bottom into a coverage that multiplies the picture's alpha.
//!
//! Tracking a mask to the picture is a placeholder for now: the field
//! is kept ([`Mask::tracked`]) so a later stage can fill it.

use std::collections::BTreeMap;

use sue_core::{Time, Value};
use sue_model::Clip;

use super::pixels::{Image, blur_plane, distance};

/// A mask's shape, in the picture's own space (0 to 1 each way).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A rectangle: its top left corner and its size.
    Rect {
        /// Top left.
        at: [f64; 2],
        /// Width and height.
        size: [f64; 2],
    },
    /// An ellipse: its middle and its radii.
    Ellipse {
        /// The middle.
        center: [f64; 2],
        /// Half its width and height.
        radius: [f64; 2],
    },
    /// A closed Bezier path.
    Path(Vec<PathPoint>),
}

/// A point of a Bezier path with its handles (offsets from it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint {
    /// Where.
    pub at: [f64; 2],
    /// The handle the curve arrives along.
    pub incoming: [f64; 2],
    /// The handle the curve leaves along.
    pub outgoing: [f64; 2],
}

impl PathPoint {
    /// A corner point (no handles).
    #[must_use]
    pub fn corner(at: [f64; 2]) -> Self {
        Self {
            at,
            incoming: [0.0, 0.0],
            outgoing: [0.0, 0.0],
        }
    }
}

/// How a mask combines with the ones above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Adds its area.
    Add,
    /// Takes its area away.
    Subtract,
    /// Keeps only where both are.
    Intersect,
}

impl Mode {
    /// Every mode.
    pub const ALL: [Self; 3] = [Self::Add, Self::Subtract, Self::Intersect];

    /// Its stable name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Intersect => "intersect",
        }
    }

    fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|mode| mode.id() == id)
            .unwrap_or(Self::Add)
    }
}

/// One mask.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    /// Its shape.
    pub shape: Shape,
    /// How it combines.
    pub mode: Mode,
    /// The soft edge, in source pixels.
    pub feather: f64,
    /// Grown by (shrunk below 0), in source pixels.
    pub expansion: f64,
    /// 0 to 1.
    pub opacity: f64,
    /// Its outside instead of its inside.
    pub inverted: bool,
    /// Follows the picture's motion (tracking comes in a later stage; the
    /// mask stays where it is).
    pub tracked: bool,
}

impl Mask {
    /// An ellipse in the middle, the way a new mask starts.
    #[must_use]
    pub fn ellipse() -> Self {
        Self::of(Shape::Ellipse {
            center: [0.5, 0.5],
            radius: [0.3, 0.3],
        })
    }

    /// A rectangle in the middle.
    #[must_use]
    pub fn rect() -> Self {
        Self::of(Shape::Rect {
            at: [0.25, 0.25],
            size: [0.5, 0.5],
        })
    }

    /// A mask of `shape` with nothing else set.
    #[must_use]
    pub fn of(shape: Shape) -> Self {
        Self {
            shape,
            mode: Mode::Add,
            feather: 0.0,
            expansion: 0.0,
            opacity: 1.0,
            inverted: false,
            tracked: false,
        }
    }

    /// As it is stored.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = BTreeMap::new();
        let (kind, points) = match &self.shape {
            Shape::Rect { at, size } => ("rect", vec![Value::Vec2(*at), Value::Vec2(*size)]),
            Shape::Ellipse { center, radius } => {
                ("ellipse", vec![Value::Vec2(*center), Value::Vec2(*radius)])
            }
            Shape::Path(points) => (
                "path",
                points
                    .iter()
                    .flat_map(|point| {
                        [
                            Value::Vec2(point.at),
                            Value::Vec2(point.incoming),
                            Value::Vec2(point.outgoing),
                        ]
                    })
                    .collect(),
            ),
        };
        map.insert("shape".to_owned(), Value::Text(kind.to_owned()));
        map.insert("points".to_owned(), Value::List(points));
        map.insert("mode".to_owned(), Value::Text(self.mode.id().to_owned()));
        map.insert("feather".to_owned(), Value::Float(self.feather));
        map.insert("expansion".to_owned(), Value::Float(self.expansion));
        map.insert("opacity".to_owned(), Value::Float(self.opacity));
        map.insert("inverted".to_owned(), Value::Bool(self.inverted));
        map.insert("tracked".to_owned(), Value::Bool(self.tracked));
        Value::Map(map)
    }

    /// Read back from what is stored.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        let Value::Map(map) = value else {
            return None;
        };
        let points: Vec<[f64; 2]> = match map.get("points") {
            Some(Value::List(list)) => list
                .iter()
                .filter_map(|value| match value {
                    Value::Vec2(point) => Some(*point),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let shape = match map.get("shape").and_then(Value::as_str)? {
            "rect" if points.len() >= 2 => Shape::Rect {
                at: points[0],
                size: points[1],
            },
            "ellipse" if points.len() >= 2 => Shape::Ellipse {
                center: points[0],
                radius: points[1],
            },
            "path" => Shape::Path(
                points
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|triple| PathPoint {
                        at: triple[0],
                        incoming: triple[1],
                        outgoing: triple[2],
                    })
                    .collect(),
            ),
            _ => return None,
        };
        let number =
            |key: &str, default: f64| map.get(key).and_then(Value::as_f64).unwrap_or(default);
        let flag = |key: &str| map.get(key).and_then(Value::as_bool).unwrap_or(false);
        Some(Self {
            shape,
            mode: Mode::from_id(map.get("mode").and_then(Value::as_str).unwrap_or("add")),
            feather: number("feather", 0.0),
            expansion: number("expansion", 0.0),
            opacity: number("opacity", 1.0),
            inverted: flag("inverted"),
            tracked: flag("tracked"),
        })
    }

    /// The outline as a polygon in pixels of a `width` by `height`
    /// picture.
    #[must_use]
    pub fn polygon(&self, width: f64, height: f64) -> Vec<[f64; 2]> {
        let to = |[x, y]: [f64; 2]| [x * width, y * height];
        match &self.shape {
            Shape::Rect { at, size } => vec![
                to(*at),
                to([at[0] + size[0], at[1]]),
                to([at[0] + size[0], at[1] + size[1]]),
                to([at[0], at[1] + size[1]]),
            ],
            Shape::Ellipse { center, radius } => {
                let steps = 96;
                (0..steps)
                    .map(|step| {
                        let angle = std::f64::consts::TAU * f64::from(step) / f64::from(steps);
                        to([
                            center[0] + radius[0] * angle.cos(),
                            center[1] + radius[1] * angle.sin(),
                        ])
                    })
                    .collect()
            }
            Shape::Path(points) => {
                let mut out = Vec::new();
                for (index, point) in points.iter().enumerate() {
                    let next = points[(index + 1) % points.len()];
                    let p0 = point.at;
                    let p1 = [p0[0] + point.outgoing[0], p0[1] + point.outgoing[1]];
                    let p3 = next.at;
                    let p2 = [p3[0] + next.incoming[0], p3[1] + next.incoming[1]];
                    let straight = point.outgoing == [0.0, 0.0] && next.incoming == [0.0, 0.0];
                    let steps = if straight { 1 } else { 24 };
                    for step in 0..steps {
                        let t = f64::from(step) / f64::from(steps);
                        let u = 1.0 - t;
                        let b = [
                            u * u * u * p0[0]
                                + 3.0 * u * u * t * p1[0]
                                + 3.0 * u * t * t * p2[0]
                                + t * t * t * p3[0],
                            u * u * u * p0[1]
                                + 3.0 * u * u * t * p1[1]
                                + 3.0 * u * t * t * p2[1]
                                + t * t * t * p3[1],
                        ];
                        out.push(to(b));
                    }
                }
                out
            }
        }
    }
}

/// The animation channel of mask `index`'s `param` (feather, expansion,
/// opacity).
#[must_use]
pub fn channel(index: usize, param: &str) -> String {
    format!("mask.{index}.{param}")
}

/// The masks on `clip`.
#[must_use]
pub fn masks(clip: &Clip) -> Vec<Mask> {
    match clip.properties.get(super::key::MASKS) {
        Some(Value::List(list)) => list.iter().filter_map(Mask::from_value).collect(),
        _ => Vec::new(),
    }
}

/// Sets `clip`'s masks (none removes the property).
pub fn set_masks(clip: &mut Clip, masks: &[Mask]) {
    if masks.is_empty() {
        clip.properties.remove(super::key::MASKS);
    } else {
        clip.properties.set(
            super::key::MASKS,
            Value::List(masks.iter().map(Mask::to_value).collect()),
        );
    }
}

/// `clip`'s masks at sequence time `time`, their keyframed numbers
/// sampled.
#[must_use]
pub fn masks_at(clip: &Clip, time: Time) -> Vec<Mask> {
    let source = clip.source_time(time);
    masks(clip)
        .into_iter()
        .enumerate()
        .map(|(index, mut mask)| {
            for (param, value) in [
                ("feather", &mut mask.feather),
                ("expansion", &mut mask.expansion),
                ("opacity", &mut mask.opacity),
            ] {
                if let Some(keyed) = clip
                    .animation
                    .channel(&channel(index, param))
                    .and_then(|curve| curve.sample(source))
                {
                    *value = keyed;
                }
            }
            mask
        })
        .collect()
}

/// How much of each pixel a polygon covers, with four rows of samples a
/// pixel and exact spans across (even odd rule).
#[must_use]
pub fn fill(polygon: &[[f64; 2]], width: usize, height: usize) -> Vec<f32> {
    let mut cover = vec![0.0_f32; width * height];
    if polygon.len() < 3 || width == 0 || height == 0 {
        return cover;
    }
    const SUB: usize = 4;
    let mut crossings: Vec<f64> = Vec::new();
    for y in 0..height {
        let row = &mut cover[y * width..(y + 1) * width];
        for sub in 0..SUB {
            let sy = y as f64 + (sub as f64 + 0.5) / SUB as f64;
            crossings.clear();
            for (index, a) in polygon.iter().enumerate() {
                let b = polygon[(index + 1) % polygon.len()];
                if (a[1] <= sy) != (b[1] <= sy) {
                    let t = (sy - a[1]) / (b[1] - a[1]);
                    crossings.push(a[0] + (b[0] - a[0]) * t);
                }
            }
            crossings.sort_by(f64::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                let (x0, x1) = (pair[0].max(0.0), pair[1].min(width as f64));
                if x1 <= x0 {
                    continue;
                }
                let (first, last) = (x0.floor() as usize, (x1.ceil() as usize).min(width));
                for (x, value) in row.iter_mut().enumerate().take(last).skip(first) {
                    let left = (x as f64).max(x0);
                    let right = ((x + 1) as f64).min(x1);
                    if right > left {
                        *value += ((right - left) / SUB as f64) as f32;
                    }
                }
            }
        }
    }
    for value in &mut cover {
        *value = value.clamp(0.0, 1.0);
    }
    cover
}

/// One mask's coverage of a `width` by `height` picture whose pixels are
/// `scale` source pixels each: filled, grown or shrunk, feathered,
/// inverted and faded by its opacity.
#[must_use]
pub fn coverage(mask: &Mask, width: usize, height: usize, scale: f32) -> Vec<f32> {
    let mut cover = fill(&mask.polygon(width as f64, height as f64), width, height);
    let grow = mask.expansion as f32 * scale;
    if grow.abs() >= 0.5 {
        if grow > 0.0 {
            let inside: Vec<bool> = cover.iter().map(|c| *c >= 0.5).collect();
            let far = distance(&inside, width, height);
            for (value, d) in cover.iter_mut().zip(far) {
                *value = value.max((grow - d + 0.5).clamp(0.0, 1.0));
            }
        } else {
            let outside: Vec<bool> = cover.iter().map(|c| *c < 0.5).collect();
            let deep = distance(&outside, width, height);
            for (value, d) in cover.iter_mut().zip(deep) {
                *value = value.min((d + grow + 0.5).clamp(0.0, 1.0));
            }
        }
    }
    let feather = mask.feather as f32 * scale;
    if feather >= 0.25 {
        cover = blur_plane(&cover, width, height, feather);
    }
    let opacity = mask.opacity.clamp(0.0, 1.0) as f32;
    for value in &mut cover {
        let shown = if mask.inverted { 1.0 - *value } else { *value };
        *value = shown * opacity;
    }
    cover
}

/// The masks combined top to bottom: the first that adds starts from
/// nothing; if none adds, from everything.
#[must_use]
pub fn combined(masks: &[Mask], width: usize, height: usize, scale: f32) -> Option<Vec<f32>> {
    if masks.is_empty() {
        return None;
    }
    let start = if masks.iter().any(|mask| mask.mode == Mode::Add) {
        0.0
    } else {
        1.0
    };
    let mut total = vec![start; width * height];
    for mask in masks {
        let cover = coverage(mask, width, height, scale);
        for (value, m) in total.iter_mut().zip(cover) {
            *value = match mask.mode {
                Mode::Add => *value + m - *value * m,
                Mode::Subtract => *value * (1.0 - m),
                Mode::Intersect => *value * m,
            };
        }
    }
    Some(total)
}

/// Multiplies `image`'s alpha by `cover`.
pub fn cut(image: &mut Image, cover: &[f32]) {
    for (pixel, value) in image.rgba.as_chunks_mut::<4>().0.iter_mut().zip(cover) {
        pixel[3] = (f32::from(pixel[3]) * value.clamp(0.0, 1.0)).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_covers_exactly_its_pixels_with_soft_edges() {
        let mask = Mask::of(Shape::Rect {
            at: [0.25, 0.25],
            size: [0.5, 0.5],
        });
        let cover = coverage(&mask, 8, 8, 1.0);
        assert_eq!(cover[3 * 8 + 3], 1.0);
        assert_eq!(cover[0], 0.0);
        let total: f32 = cover.iter().sum();
        assert!((total - 16.0).abs() < 1e-3, "{total}");
        // Half a pixel off the grid: half covered edges.
        let half = fill(&[[1.5, 1.0], [3.0, 1.0], [3.0, 3.0], [1.5, 3.0]], 4, 4);
        assert!((half[4 + 1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn an_ellipse_covers_its_area_and_inverts() {
        let mask = Mask::ellipse();
        let (w, h) = (100, 100);
        let cover = coverage(&mask, w, h, 1.0);
        let area: f32 = cover.iter().sum();
        let expected = std::f32::consts::PI * 30.0 * 30.0;
        assert!(
            (area - expected).abs() / expected < 0.01,
            "{area} for {expected}"
        );
        let mut inverted = mask;
        inverted.inverted = true;
        let outside: f32 = coverage(&inverted, w, h, 1.0).iter().sum();
        assert!((outside + area - 10_000.0).abs() < 1.0);
    }

    #[test]
    fn expansion_grows_feather_softens_and_modes_combine() {
        let mut mask = Mask::rect();
        let (w, h) = (40, 40);
        let base: f32 = coverage(&mask, w, h, 1.0).iter().sum();
        mask.expansion = 2.0;
        let grown: f32 = coverage(&mask, w, h, 1.0).iter().sum();
        assert!(grown > base + 100.0, "{grown} {base}");
        mask.expansion = -2.0;
        let shrunk: f32 = coverage(&mask, w, h, 1.0).iter().sum();
        assert!(shrunk < base - 100.0);
        mask.expansion = 0.0;
        mask.feather = 4.0;
        let soft = coverage(&mask, w, h, 1.0);
        let edge = soft[20 * w + 10];
        assert!(edge > 0.2 && edge < 0.8, "{edge}");
        // Add then subtract a smaller one: a frame.
        let mut hole = Mask::of(Shape::Rect {
            at: [0.375, 0.375],
            size: [0.25, 0.25],
        });
        hole.mode = Mode::Subtract;
        let both = combined(&[Mask::rect(), hole], w, h, 1.0).unwrap();
        assert_eq!(both[20 * w + 20], 0.0);
        assert_eq!(both[12 * w + 12], 1.0);
    }

    #[test]
    fn masks_round_trip_and_sample_their_keys() {
        let mut path = Mask::of(Shape::Path(vec![
            PathPoint::corner([0.1, 0.1]),
            PathPoint {
                at: [0.9, 0.1],
                incoming: [-0.1, 0.0],
                outgoing: [0.0, 0.2],
            },
            PathPoint::corner([0.5, 0.9]),
        ]));
        path.mode = Mode::Intersect;
        path.feather = 3.0;
        for mask in [Mask::ellipse(), Mask::rect(), path] {
            assert_eq!(Mask::from_value(&mask.to_value()), Some(mask));
        }
        let mut clip = Clip::new(sue_core::EntityId::new(), Time::ZERO, Time::SECOND);
        set_masks(&mut clip, &[Mask::ellipse()]);
        let curve = clip.animation.channel_mut(&channel(0, "feather"));
        curve.insert(sue_model::Key {
            time: Time::ZERO,
            value: 0.0,
            interpolation: sue_model::Interpolation::Linear,
        });
        curve.insert(sue_model::Key {
            time: Time::SECOND,
            value: 10.0,
            interpolation: sue_model::Interpolation::Linear,
        });
        let half = masks_at(&clip, Time::from_seconds(0.5));
        assert!((half[0].feather - 5.0).abs() < 1e-9);
    }
}
