//! Rulers, as in Clip Studio Paint: the linear ruler, special rulers
//! (parallel lines, radial lines, concentric circles), the perspective
//! ruler with one to three vanishing points, and the symmetrical ruler.
//! Each is kept on the picture as text, so it undoes, saves and opens
//! again with it.
//!
//! A stroke follows them while snapping is on (Snap to Ruler for the
//! linear ruler, Snap to Special Ruler for the rest): once the pen has
//! moved a little, the guide through where it began that runs most like
//! the pen is chosen, and every point after is put on it.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, Value};

use crate::doc::Edit;

/// The linear ruler: its two ends, `x0,y0,x1,y1` in picture pixels.
pub const LINEAR: &str = "ruler.linear";
/// The special ruler ([`Special::to_text`]).
pub const SPECIAL: &str = "ruler.special";
/// The perspective ruler's vanishing points, `x,y;x,y;x,y`.
pub const PERSPECTIVE: &str = "ruler.perspective";
/// The symmetrical ruler: its center and angle (radians), `x,y,angle`.
pub const SYMMETRY: &str = "ruler.symmetry";
/// Curve, figure and pen rulers: paths joined by `|`, each `o:` (open) or
/// `c:` (closed) and its points `x,y x,y ...`.
pub const PATHS: &str = "ruler.paths";
/// Every ruler a picture keeps.
pub const ALL: [&str; 5] = [LINEAR, SPECIAL, PERSPECTIVE, SYMMETRY, PATHS];

/// A ruler along a path: a curve ruler, a figure ruler or one drawn with
/// the ruler pen (picture pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    /// Its points, close enough together to follow.
    pub points: Vec<[f32; 2]>,
    /// The last point joins the first.
    pub closed: bool,
}

impl Path {
    /// A curve ruler through `points`: a smooth curve (Catmull-Rom) sampled
    /// every 2 pixels.
    #[must_use]
    pub fn curve(points: &[[f32; 2]]) -> Self {
        let mut out = Vec::new();
        for index in 0..points.len().saturating_sub(1) {
            let p1 = points[index];
            let p2 = points[index + 1];
            let p0 = if index == 0 { p1 } else { points[index - 1] };
            let p3 = points.get(index + 2).copied().unwrap_or(p2);
            let steps = (distance(p1, p2) / 2.0).ceil().max(1.0) as usize;
            for step in 0..steps {
                let u = step as f32 / steps as f32;
                let (u2, u3) = (u * u, u * u * u);
                out.push([0, 1].map(|axis| {
                    0.5 * (2.0 * p1[axis]
                        + (p2[axis] - p0[axis]) * u
                        + (2.0 * p0[axis] - 5.0 * p1[axis] + 4.0 * p2[axis] - p3[axis]) * u2
                        + (3.0 * p1[axis] - p0[axis] - 3.0 * p2[axis] + p3[axis]) * u3)
                }));
            }
        }
        out.extend(points.last());
        Self {
            points: out,
            closed: false,
        }
    }

    /// A figure ruler: the rectangle or (with `ellipse`) the ellipse in the
    /// box from `a` to `b`.
    #[must_use]
    pub fn figure(a: [f32; 2], b: [f32; 2], ellipse: bool) -> Self {
        let (min, max) = (
            [a[0].min(b[0]), a[1].min(b[1])],
            [a[0].max(b[0]), a[1].max(b[1])],
        );
        let points = if ellipse {
            let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
            let radius = [(max[0] - min[0]) / 2.0, (max[1] - min[1]) / 2.0];
            (0..96)
                .map(|step| {
                    let theta = step as f32 * std::f32::consts::TAU / 96.0;
                    [
                        center[0] + radius[0] * theta.cos(),
                        center[1] + radius[1] * theta.sin(),
                    ]
                })
                .collect()
        } else {
            vec![min, [max[0], min[1]], max, [min[0], max[1]]]
        };
        Self {
            points,
            closed: true,
        }
    }

    fn text(&self) -> String {
        let points: Vec<String> = self
            .points
            .iter()
            .map(|[x, y]| format!("{x},{y}"))
            .collect();
        format!(
            "{}:{}",
            if self.closed { "c" } else { "o" },
            points.join(" ")
        )
    }

    fn from_text(text: &str) -> Option<Self> {
        let (kind, points) = text.split_once(':')?;
        let points: Vec<[f32; 2]> = points
            .split_whitespace()
            .map(|point| match numbers(point).as_slice() {
                &[x, y] => Some([x, y]),
                _ => None,
            })
            .collect::<Option<_>>()?;
        (points.len() >= 2).then_some(Self {
            points,
            closed: kind == "c",
        })
    }

    /// The segments to follow, the closing one too.
    fn segments(&self) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
        let closing = self
            .closed
            .then(|| (self.points[self.points.len() - 1], self.points[0]));
        self.points
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .chain(closing)
    }

    /// The point of the path nearest `p`, and the direction there.
    fn nearest(&self, p: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let mut best = (self.points[0], [1.0, 0.0], f32::INFINITY);
        for (a, b) in self.segments() {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = dx * dx + dy * dy;
            let t = if length > 0.0 {
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let at = [a[0] + dx * t, a[1] + dy * t];
            let away = distance(at, p);
            if away < best.2 {
                best = (at, unit([dx, dy]).unwrap_or([1.0, 0.0]), away);
            }
        }
        (best.0, best.1)
    }
}

fn numbers(text: &str) -> Vec<f32> {
    text.split(',')
        .filter_map(|value| value.trim().parse().ok())
        .collect()
}

/// `v` at length 1, or `None` when it has none.
fn unit([x, y]: [f32; 2]) -> Option<[f32; 2]> {
    let length = x.hypot(y);
    (length > 1e-4).then(|| [x / length, y / length])
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How many times a symmetrical ruler of `lines` turns around its middle;
/// Clip Studio Paint counts the mirrored lines among them.
#[must_use]
pub const fn turns(lines: u32, mirror: bool) -> u32 {
    let lines = if lines == 0 { 1 } else { lines };
    if mirror { lines.div_ceil(2) } else { lines }
}

/// A special ruler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Special {
    /// Lines at an angle (radians).
    Parallel(f32),
    /// Lines through a center.
    Radial([f32; 2]),
    /// Circles around a center.
    Concentric([f32; 2]),
}

impl Special {
    /// The kinds a Special Ruler sub tool lays, in order.
    pub const NAMES: [&str; 3] = ["Parallel line", "Radial line", "Concentric circle"];

    /// The ruler as text, for the picture.
    #[must_use]
    pub fn to_text(&self) -> String {
        match *self {
            Self::Parallel(angle) => format!("parallel:{angle}"),
            Self::Radial([x, y]) => format!("radial:{x},{y}"),
            Self::Concentric([x, y]) => format!("concentric:{x},{y}"),
        }
    }

    /// A ruler read back from [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let (kind, values) = text.split_once(':')?;
        match (kind, numbers(values).as_slice()) {
            ("parallel", &[angle]) => Some(Self::Parallel(angle)),
            ("radial", &[x, y]) => Some(Self::Radial([x, y])),
            ("concentric", &[x, y]) => Some(Self::Concentric([x, y])),
            _ => None,
        }
    }

    fn shifted(self, [dx, dy]: [f32; 2]) -> Self {
        match self {
            Self::Parallel(angle) => Self::Parallel(angle),
            Self::Radial([x, y]) => Self::Radial([x + dx, y + dy]),
            Self::Concentric([x, y]) => Self::Concentric([x + dx, y + dy]),
        }
    }
}

/// The perspective ruler: one to three vanishing points (picture pixels).
/// With one, lines also run level and upright; with two, upright.
#[derive(Clone, Debug, PartialEq)]
pub struct Perspective {
    /// Its vanishing points; the first two lie on the eye level.
    pub points: Vec<[f32; 2]>,
}

impl Perspective {
    /// The ruler Create Perspective Ruler lays for `count` points on a
    /// `width` by `height` canvas, as Clip Studio Paint does: the eye level
    /// across the middle, one point in the middle or two on the left and
    /// right edges, and a third far below.
    #[must_use]
    pub fn new(count: u32, [width, height]: [u32; 2]) -> Self {
        let (w, h) = (width as f32, height as f32);
        let eye = h / 2.0;
        let points = match count {
            0 | 1 => vec![[w / 2.0, eye]],
            2 => vec![[0.0, eye], [w, eye]],
            _ => vec![[0.0, eye], [w, eye], [w / 2.0, eye + h * 2.5]],
        };
        Self { points }
    }

    /// The ruler as text, for the picture.
    #[must_use]
    pub fn to_text(&self) -> String {
        self.points
            .iter()
            .map(|[x, y]| format!("{x},{y}"))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// A ruler read back from [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let points: Vec<[f32; 2]> = text
            .split(';')
            .map(|point| match numbers(point).as_slice() {
                &[x, y] => Some([x, y]),
                _ => None,
            })
            .collect::<Option<_>>()?;
        (1..=3).contains(&points.len()).then_some(Self { points })
    }

    /// The eye level: a point on it and its direction.
    #[must_use]
    pub fn eye_level(&self) -> ([f32; 2], [f32; 2]) {
        let first = self.points[0];
        let direction = self
            .points
            .get(1)
            .and_then(|second| unit([second[0] - first[0], second[1] - first[1]]))
            .unwrap_or([1.0, 0.0]);
        (first, direction)
    }

    /// The directions a line through `at` can run.
    fn directions(&self, at: [f32; 2]) -> Vec<[f32; 2]> {
        let mut out: Vec<[f32; 2]> = self
            .points
            .iter()
            .filter_map(|point| unit([point[0] - at[0], point[1] - at[1]]))
            .collect();
        let (_, level) = self.eye_level();
        let upright = [-level[1], level[0]];
        match self.points.len() {
            1 => out.extend([level, upright]),
            2 => out.push(upright),
            _ => {}
        }
        out
    }

    /// The same with another vanishing point, or `None` when it has three.
    #[must_use]
    pub fn with_point(this: Option<&Self>, point: [f32; 2]) -> Option<Self> {
        let mut points = this.map(|this| this.points.clone()).unwrap_or_default();
        (points.len() < 3).then(|| {
            points.push(point);
            Self { points }
        })
    }
}

/// Where two lines, each through two points, cross; `None` when they run
/// parallel (Add vanishing point).
#[must_use]
pub fn crossing([a, b]: [[f32; 2]; 2], [c, d]: [[f32; 2]; 2]) -> Option<[f32; 2]> {
    let (r, s) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]]);
    let cross = r[0] * s[1] - r[1] * s[0];
    let scale = r[0].hypot(r[1]) * s[0].hypot(s[1]);
    if scale <= 0.0 || cross.abs() < scale * 1e-3 {
        return None;
    }
    let t = ((c[0] - a[0]) * s[1] - (c[1] - a[1]) * s[0]) / cross;
    Some([a[0] + r[0] * t, a[1] + r[1] * t])
}

/// A line, circle or path a stroke follows.
#[derive(Clone, Debug, PartialEq)]
pub enum Guide {
    /// The line through `origin` along `direction` (length 1).
    Line {
        /// A point on it.
        origin: [f32; 2],
        /// Its direction.
        direction: [f32; 2],
    },
    /// The circle around `center`, `radius` from it.
    Circle {
        /// Its middle.
        center: [f32; 2],
        /// Its radius.
        radius: f32,
    },
    /// A curve, figure or pen ruler.
    Path(Path),
}

impl Guide {
    /// The point of the guide nearest `p`.
    #[must_use]
    pub fn project(&self, p: [f32; 2]) -> [f32; 2] {
        match *self {
            Self::Line { origin, direction } => {
                let t = (p[0] - origin[0]) * direction[0] + (p[1] - origin[1]) * direction[1];
                [origin[0] + direction[0] * t, origin[1] + direction[1] * t]
            }
            Self::Circle { center, radius } => {
                let out = unit([p[0] - center[0], p[1] - center[1]]).unwrap_or([1.0, 0.0]);
                [center[0] + out[0] * radius, center[1] + out[1] * radius]
            }
            Self::Path(ref path) => path.nearest(p).0,
        }
    }

    /// How closely the guide at `at` runs along `toward` (length 1), 0 to 1.
    fn along(&self, at: [f32; 2], toward: [f32; 2]) -> f32 {
        let tangent = match *self {
            Self::Line { direction, .. } => Some(direction),
            Self::Circle { center, .. } => {
                unit([at[0] - center[0], at[1] - center[1]]).map(|out| [-out[1], out[0]])
            }
            Self::Path(ref path) => Some(path.nearest(at).1),
        };
        tangent.map_or(0.0, |[x, y]| (x * toward[0] + y * toward[1]).abs())
    }
}

/// A picture's rulers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rulers {
    /// The linear ruler's ends.
    pub linear: Option<[[f32; 2]; 2]>,
    /// The special ruler.
    pub special: Option<Special>,
    /// The perspective ruler.
    pub perspective: Option<Perspective>,
    /// The symmetrical ruler's center and angle.
    pub symmetry: Option<([f32; 2], f32)>,
    /// Curve, figure and pen rulers.
    pub paths: Vec<Path>,
}

impl Rulers {
    /// The rulers on `picture`.
    #[must_use]
    pub fn of(arena: &Arena, picture: EntityId) -> Self {
        let Ok(entity) = arena.get_by_id(picture) else {
            return Self::default();
        };
        let text = |key: &str| entity.property(key).and_then(Value::as_str);
        Self {
            linear: text(LINEAR).and_then(|text| match numbers(text).as_slice() {
                &[x0, y0, x1, y1] => Some([[x0, y0], [x1, y1]]),
                _ => None,
            }),
            special: text(SPECIAL).and_then(Special::from_text),
            perspective: text(PERSPECTIVE).and_then(Perspective::from_text),
            symmetry: text(SYMMETRY).and_then(|text| match numbers(text).as_slice() {
                &[x, y, angle] => Some(([x, y], angle)),
                _ => None,
            }),
            paths: text(PATHS)
                .map(|text| text.split('|').filter_map(Path::from_text).collect())
                .unwrap_or_default(),
        }
    }

    /// `true` when there is no ruler.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The same rulers moved by `by` (into a layer's own pixels).
    #[must_use]
    pub fn shifted(&self, by: [f32; 2]) -> Self {
        let move_point = |[x, y]: [f32; 2]| [x + by[0], y + by[1]];
        Self {
            linear: self.linear.map(|ends| ends.map(move_point)),
            special: self.special.map(|special| special.shifted(by)),
            perspective: self.perspective.as_ref().map(|perspective| Perspective {
                points: perspective.points.iter().copied().map(move_point).collect(),
            }),
            symmetry: self
                .symmetry
                .map(|(center, angle)| (move_point(center), angle)),
            paths: self
                .paths
                .iter()
                .map(|path| Path {
                    points: path.points.iter().copied().map(move_point).collect(),
                    closed: path.closed,
                })
                .collect(),
        }
    }

    /// The point a perspective ruler's eye level is moved by, the one on it
    /// nearest the middle of a `size` canvas (for two or three points).
    #[must_use]
    pub fn eye_handle(&self, size: [u32; 2]) -> Option<[f32; 2]> {
        let perspective = self.perspective.as_ref().filter(|p| p.points.len() >= 2)?;
        let middle = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
        let (origin, direction) = perspective.eye_level();
        Some(Guide::Line { origin, direction }.project(middle))
    }

    /// The handle of a ruler within `reach` of `p`, nearest first.
    #[must_use]
    pub fn handle_at(&self, p: [f32; 2], reach: f32, size: [u32; 2]) -> Option<Handle> {
        let mut handles: Vec<(Handle, [f32; 2])> = Vec::new();
        if let Some(perspective) = &self.perspective {
            for (index, point) in perspective.points.iter().enumerate() {
                handles.push((Handle::Vanishing(index), *point));
            }
        }
        if let Some(at) = self.eye_handle(size) {
            handles.push((Handle::EyeLevel, at));
        }
        if let Some(ends) = self.linear {
            handles.push((Handle::LinearEnd(0), ends[0]));
            handles.push((Handle::LinearEnd(1), ends[1]));
        }
        handles
            .into_iter()
            .map(|(handle, at)| (handle, distance(at, p)))
            .filter(|(_, away)| *away <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(handle, _)| handle)
    }

    /// The rulers with `handle` dragged to `to`. With `fix_eye_level` the
    /// first two vanishing points slide along the eye level only.
    #[must_use]
    pub fn moved(&self, handle: Handle, to: [f32; 2], fix_eye_level: bool, size: [u32; 2]) -> Self {
        let mut out = self.clone();
        match handle {
            Handle::Vanishing(index) => {
                if let Some(perspective) = out.perspective.as_mut() {
                    let (origin, direction) = perspective.eye_level();
                    let on_level = fix_eye_level && index < 2 && perspective.points.len() >= 2;
                    if let Some(point) = perspective.points.get_mut(index) {
                        *point = if on_level {
                            Guide::Line { origin, direction }.project(to)
                        } else {
                            to
                        };
                    }
                }
            }
            Handle::EyeLevel => {
                if let (Some(from), Some(perspective)) =
                    (self.eye_handle(size), out.perspective.as_mut())
                {
                    let (_, direction) = perspective.eye_level();
                    let normal = [-direction[1], direction[0]];
                    let by = (to[0] - from[0]) * normal[0] + (to[1] - from[1]) * normal[1];
                    for point in perspective.points.iter_mut().take(2) {
                        point[0] += normal[0] * by;
                        point[1] += normal[1] * by;
                    }
                }
            }
            Handle::LinearEnd(index) => {
                if let Some(ends) = out.linear.as_mut() {
                    ends[index.min(1)] = to;
                }
            }
        }
        out
    }

    /// The text `key` holds for these rulers (`None` when there is none).
    #[must_use]
    pub fn text(&self, key: &str) -> Option<String> {
        match key {
            LINEAR => self
                .linear
                .map(|[[x0, y0], [x1, y1]]| format!("{x0},{y0},{x1},{y1}")),
            SPECIAL => self.special.map(|special| special.to_text()),
            PERSPECTIVE => self.perspective.as_ref().map(Perspective::to_text),
            SYMMETRY => self
                .symmetry
                .map(|([x, y], angle)| format!("{x},{y},{angle}")),
            PATHS => (!self.paths.is_empty()).then(|| {
                self.paths
                    .iter()
                    .map(Path::text)
                    .collect::<Vec<_>>()
                    .join("|")
            }),
            _ => None,
        }
    }
}

/// A part of a ruler that can be dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// A perspective ruler's vanishing point.
    Vanishing(usize),
    /// A perspective ruler's eye level.
    EyeLevel,
    /// One end of the linear ruler.
    LinearEnd(usize),
}

impl Handle {
    /// The picture property it changes.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Vanishing(_) | Self::EyeLevel => PERSPECTIVE,
            Self::LinearEnd(_) => LINEAR,
        }
    }
}

/// The edit that sets `key` of `picture` to `value`, or takes it away for
/// `None`, named `label`.
#[must_use]
pub fn edit(picture: EntityId, key: &str, value: Option<String>, label: &str) -> Edit {
    Edit::new(label).then(match value {
        Some(value) => SetProperty::new(picture, key, value),
        None => SetProperty::remove(picture, key),
    })
}

/// A stroke following rulers: still choosing its guide, then on it.
#[derive(Clone, Debug, PartialEq)]
pub enum Snap {
    /// Waiting for the pen to move from `start` to choose among `guides`.
    Choosing {
        /// Where the stroke began.
        start: [f32; 2],
        /// The guides through it.
        guides: Vec<Guide>,
    },
    /// Following its guide.
    On(Guide),
}

impl Snap {
    /// A stroke beginning at `start` (layer pixels, the layer at
    /// `offset`) within `reach` of a line of the grid every `step`
    /// picture pixels follows that line (View, Snap to Grid); near a
    /// crossing it goes along whichever line the pen heads down.
    #[must_use]
    pub fn grid(start: [f32; 2], offset: [f32; 2], step: f32, reach: f32) -> Option<Self> {
        if step <= 0.0 {
            return None;
        }
        let picture = [start[0] + offset[0], start[1] + offset[1]];
        let near = |value: f32| (value / step).round() * step;
        let (x, y) = (near(picture[0]), near(picture[1]));
        let mut guides = Vec::new();
        if (picture[0] - x).abs() <= reach {
            guides.push(Guide::Line {
                origin: [x - offset[0], start[1]],
                direction: [0.0, 1.0],
            });
        }
        if (picture[1] - y).abs() <= reach {
            guides.push(Guide::Line {
                origin: [start[0], y - offset[1]],
                direction: [1.0, 0.0],
            });
        }
        match guides.len() {
            0 => None,
            1 => guides.pop().map(Self::On),
            _ => Some(Self::Choosing { start, guides }),
        }
    }

    /// A stroke beginning at `start` following `rulers`: the linear ruler
    /// with `ruler` on when it starts within `reach` of it, the others with
    /// `special` on. `None` when there is nothing to follow.
    #[must_use]
    pub fn begin(
        rulers: &Rulers,
        start: [f32; 2],
        ruler: bool,
        special: bool,
        reach: f32,
    ) -> Option<Self> {
        if ruler
            && let Some([a, b]) = rulers.linear
            && let Some(direction) = unit([b[0] - a[0], b[1] - a[1]])
        {
            let line = Guide::Line {
                origin: a,
                direction,
            };
            if distance(line.project(start), start) <= reach {
                return Some(Self::On(line));
            }
        }
        if ruler {
            // The nearest curve, figure or pen ruler the stroke starts on.
            let near = rulers
                .paths
                .iter()
                .map(|path| (path, distance(path.nearest(start).0, start)))
                .filter(|(_, away)| *away <= reach)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((path, _)) = near {
                return Some(Self::On(Guide::Path(path.clone())));
            }
        }
        if !special {
            return None;
        }
        let mut guides: Vec<Guide> = rulers
            .perspective
            .iter()
            .flat_map(|perspective| perspective.directions(start))
            .map(|direction| Guide::Line {
                origin: start,
                direction,
            })
            .collect();
        match rulers.special {
            Some(Special::Parallel(angle)) => guides.push(Guide::Line {
                origin: start,
                direction: [angle.cos(), angle.sin()],
            }),
            Some(Special::Radial(center)) => {
                if let Some(direction) = unit([center[0] - start[0], center[1] - start[1]]) {
                    guides.push(Guide::Line {
                        origin: start,
                        direction,
                    });
                }
            }
            Some(Special::Concentric(center)) => {
                let radius = distance(start, center);
                if radius >= 1.0 {
                    guides.push(Guide::Circle { center, radius });
                }
            }
            None => {}
        }
        match guides.len() {
            0 => None,
            1 => guides.pop().map(Self::On),
            _ => Some(Self::Choosing { start, guides }),
        }
    }

    /// `p` on the guide, choosing the guide once the pen is `reach` from
    /// where it began; `None` while it is still choosing.
    pub fn follow(&mut self, p: [f32; 2], reach: f32) -> Option<[f32; 2]> {
        match self {
            Self::On(guide) => Some(guide.project(p)),
            Self::Choosing { start, guides } => {
                let toward = unit([p[0] - start[0], p[1] - start[1]])
                    .filter(|_| distance(p, *start) >= reach)?;
                let at = *start;
                let guide = guides
                    .iter()
                    .max_by(|a, b| a.along(at, toward).total_cmp(&b.along(at, toward)))?
                    .clone();
                let on = guide.project(p);
                *self = Self::On(guide);
                Some(on)
            }
        }
    }

    /// `p` on the guide once there is one.
    #[must_use]
    pub fn project(&self, p: [f32; 2]) -> [f32; 2] {
        match self {
            Self::On(guide) => guide.project(p),
            Self::Choosing { .. } => p,
        }
    }

    /// `true` once the guide is chosen.
    #[must_use]
    pub const fn chosen(&self) -> bool {
        matches!(self, Self::On(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        distance(a, b) < 1e-2
    }

    #[test]
    fn strokes_near_a_grid_line_follow_it() {
        let mut snap = Snap::grid([62.0, 10.0], [0.0, 0.0], 64.0, 4.0).unwrap();
        assert_eq!(
            snap.follow([70.0, 40.0], 2.0),
            Some([64.0, 40.0]),
            "down the line at x 64"
        );
        assert!(
            Snap::grid([30.0, 30.0], [0.0, 0.0], 64.0, 4.0).is_none(),
            "far from every line"
        );
        // Near a crossing, the way the pen goes picks the line.
        let mut snap = Snap::grid([63.0, 65.0], [0.0, 0.0], 64.0, 4.0).unwrap();
        assert_eq!(snap.follow([90.0, 66.0], 2.0), Some([90.0, 64.0]));
    }

    #[test]
    fn rulers_read_back_from_their_text() {
        for special in [
            Special::Parallel(0.5),
            Special::Radial([10.0, -4.5]),
            Special::Concentric([3.0, 7.0]),
        ] {
            assert_eq!(Special::from_text(&special.to_text()), Some(special));
        }
        let three = Perspective::new(3, [400, 300]);
        assert_eq!(
            three.points,
            vec![[0.0, 150.0], [400.0, 150.0], [200.0, 900.0]]
        );
        assert_eq!(Perspective::from_text(&three.to_text()), Some(three));
        assert_eq!(Perspective::from_text(""), None);
        assert_eq!(
            Perspective::from_text("1,2;3,4;5,6;7,8"),
            None,
            "at most three"
        );
        let rulers = Rulers {
            linear: Some([[1.0, 2.0], [3.0, 4.0]]),
            symmetry: Some(([5.0, 6.0], 1.5)),
            ..Rulers::default()
        };
        let mut arena = Arena::new();
        let picture = sue_core::Entity::empty("Picture", "sue.image");
        let id = picture.id();
        arena.insert_keep_id(picture).unwrap();
        for key in ALL {
            if let Some(text) = rulers.text(key) {
                let mut set = edit(id, key, Some(text), "Ruler");
                sue_core::ReversibleTransaction::apply(&mut set, &mut arena).unwrap();
            }
        }
        assert_eq!(Rulers::of(&arena, id), rulers);
        let mut gone = edit(id, LINEAR, None, "Delete Ruler");
        sue_core::ReversibleTransaction::apply(&mut gone, &mut arena).unwrap();
        assert_eq!(Rulers::of(&arena, id).linear, None);
        sue_core::ReversibleTransaction::revert(&mut gone, &mut arena).unwrap();
        assert_eq!(Rulers::of(&arena, id), rulers, "and back on undo");
    }

    #[test]
    fn strokes_follow_the_vanishing_point_they_run_toward() {
        let rulers = Rulers {
            perspective: Some(Perspective::new(2, [400, 300])),
            ..Rulers::default()
        };
        let start = [200.0, 50.0];
        // Heading down and right: toward the right vanishing point.
        let mut snap = Snap::begin(&rulers, start, true, true, 10.0).unwrap();
        assert!(!snap.chosen());
        assert_eq!(snap.follow([201.0, 50.5], 4.0), None, "still choosing");
        let on = snap.follow([240.0, 62.0], 4.0).unwrap();
        assert!(snap.chosen());
        let toward = Guide::Line {
            origin: start,
            direction: unit([200.0, 100.0]).unwrap(),
        };
        assert!(near(on, toward.project([240.0, 62.0])));
        let far = snap.follow([380.0, 145.0], 4.0).unwrap();
        assert!(near(toward.project(far), far), "every point after is on it");
        // Heading straight down: the upright of two-point perspective.
        let mut down = Snap::begin(&rulers, start, true, true, 10.0).unwrap();
        let below = down.follow([203.0, 120.0], 4.0).unwrap();
        assert!(near(below, [200.0, 120.0]));
        // Without Snap to Special Ruler there is nothing to follow.
        assert_eq!(Snap::begin(&rulers, start, true, false, 10.0), None);
    }

    #[test]
    fn special_and_linear_rulers_hold_strokes_on_them() {
        let circle = Rulers {
            special: Some(Special::Concentric([0.0, 0.0])),
            ..Rulers::default()
        };
        let mut snap = Snap::begin(&circle, [10.0, 0.0], true, true, 5.0).unwrap();
        let p = snap.follow([0.0, 20.0], 2.0).unwrap();
        assert!(near(p, [0.0, 10.0]), "on the circle it began on");
        let radial = Rulers {
            special: Some(Special::Radial([0.0, 0.0])),
            ..Rulers::default()
        };
        let mut snap = Snap::begin(&radial, [10.0, 10.0], true, true, 5.0).unwrap();
        assert!(near(snap.follow([20.0, 22.0], 2.0).unwrap(), [21.0, 21.0]));
        let linear = Rulers {
            linear: Some([[0.0, 0.0], [100.0, 0.0]]),
            ..Rulers::default()
        };
        let mut snap = Snap::begin(&linear, [30.0, 6.0], true, true, 8.0).unwrap();
        assert!(
            near(snap.project([30.0, 6.0]), [30.0, 0.0]),
            "onto the ruler at once"
        );
        assert!(near(snap.follow([70.0, 9.0], 2.0).unwrap(), [70.0, 0.0]));
        assert_eq!(
            Snap::begin(&linear, [30.0, 40.0], true, true, 8.0),
            None,
            "too far from it"
        );
        assert_eq!(Snap::begin(&linear, [30.0, 6.0], false, true, 8.0), None);
    }

    #[test]
    fn curve_figure_and_pen_rulers_hold_strokes_that_start_on_them() {
        let curve = Path::curve(&[[0.0, 0.0], [50.0, 30.0], [100.0, 0.0]]);
        assert!(curve.points.len() > 20);
        assert!(near(*curve.points.last().unwrap(), [100.0, 0.0]));
        let rulers = Rulers {
            paths: vec![curve, Path::figure([200.0, 200.0], [300.0, 260.0], false)],
            ..Rulers::default()
        };
        let mut arena = Arena::new();
        let picture = sue_core::Entity::empty("Picture", "sue.image");
        let id = picture.id();
        arena.insert_keep_id(picture).unwrap();
        let mut set = edit(id, PATHS, rulers.text(PATHS), "Ruler");
        sue_core::ReversibleTransaction::apply(&mut set, &mut arena).unwrap();
        let back = Rulers::of(&arena, id);
        assert_eq!(back.paths.len(), 2);
        assert!(back.paths[1].closed);
        // A stroke starting on the curve stays on it.
        let mut snap = Snap::begin(&back, [50.0, 31.0], true, true, 5.0).unwrap();
        let p = snap.follow([52.0, 60.0], 2.0).unwrap();
        assert!((p[1] - 30.0).abs() < 2.0, "{p:?}");
        // One on the rectangle's edge follows the edge round the corner.
        let mut snap = Snap::begin(&back, [250.0, 201.0], true, true, 5.0).unwrap();
        assert!(near(
            snap.follow([320.0, 230.0], 2.0).unwrap(),
            [300.0, 230.0]
        ));
        // Far from every ruler, or with Snap to Ruler off: free.
        assert_eq!(Snap::begin(&back, [150.0, 150.0], true, true, 5.0), None);
        assert_eq!(Snap::begin(&back, [50.0, 31.0], false, true, 5.0), None);
    }

    #[test]
    fn vanishing_points_come_from_two_lines_and_move_by_their_handles() {
        let point = crossing([[0.0, 0.0], [10.0, 5.0]], [[0.0, 20.0], [10.0, 15.0]]).unwrap();
        assert!(near(point, [20.0, 10.0]));
        assert_eq!(
            crossing([[0.0, 0.0], [10.0, 0.0]], [[0.0, 5.0], [10.0, 5.0]]),
            None
        );
        let one = Perspective::with_point(None, point).unwrap();
        let two = Perspective::with_point(Some(&one), [-50.0, 10.0]).unwrap();
        let three = Perspective::with_point(Some(&two), [0.0, 500.0]).unwrap();
        assert_eq!(Perspective::with_point(Some(&three), [1.0, 1.0]), None);
        let rulers = Rulers {
            perspective: Some(Perspective::new(2, [400, 300])),
            ..Rulers::default()
        };
        let size = [400, 300];
        assert_eq!(
            rulers.handle_at([398.0, 151.0], 6.0, size),
            Some(Handle::Vanishing(1))
        );
        assert_eq!(
            rulers.handle_at([200.0, 152.0], 6.0, size),
            Some(Handle::EyeLevel)
        );
        assert_eq!(rulers.handle_at([200.0, 40.0], 6.0, size), None);
        // A fixed eye level: the point slides along it.
        let slid = rulers.moved(Handle::Vanishing(1), [450.0, 100.0], true, size);
        assert_eq!(slid.perspective.unwrap().points[1], [450.0, 150.0]);
        let free = rulers.moved(Handle::Vanishing(1), [450.0, 100.0], false, size);
        assert_eq!(free.perspective.unwrap().points[1], [450.0, 100.0]);
        // The eye level carries both points with it.
        let raised = rulers.moved(Handle::EyeLevel, [200.0, 90.0], true, size);
        assert!(
            raised
                .perspective
                .unwrap()
                .points
                .iter()
                .all(|point| (point[1] - 90.0).abs() < 1e-3)
        );
    }
}
