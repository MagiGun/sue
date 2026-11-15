//! The gradient engine: how a gradient of stops ([`GradientMap`]) is laid
//! over pixels by the Gradient tool, drawn by a gradient layer, and
//! followed round an area's outline by Contour line fill.
//!
//! A gradient runs along a line from its start to its end. Its
//! [`Shape`] says how far along a pixel lies: across the line (Linear),
//! out from the start (Circle), or out from the start on an ellipse whose
//! long axis is the line and whose short axis is [`Look::aspect`] of it
//! (Ellipse). Its [`Edge`] says what happens before the start and past
//! the end: the end colors carry on, the gradient repeats, it repeats
//! back and forth, or nothing is drawn there.

use sue_model::BlendMode;

use crate::gradient_map::{GradientMap, StopColor};
use crate::select::Selection;

/// How far along a gradient a pixel lies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Straight across the line, as bands at right angles to it.
    #[default]
    Linear,
    /// Out from the start, in rings.
    Circle,
    /// Out from the start, in ellipses along the line.
    Ellipse,
}

impl Shape {
    /// Every shape, in the order Tool Property offers them.
    pub const ALL: [Self; 3] = [Self::Linear, Self::Circle, Self::Ellipse];

    /// Its stable name in settings and documents.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Circle => "circle",
            Self::Ellipse => "ellipse",
        }
    }

    /// The shape named `id` ([`Self::id`]).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shape| shape.id() == id)
    }
}

/// What a gradient does before its start and past its end (Edge
/// process).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Edge {
    /// The end colors carry on (Do not repeat).
    #[default]
    Extend,
    /// The gradient starts again (Repeat).
    Repeat,
    /// The gradient runs back and forth (Reverse).
    Mirror,
    /// Nothing is drawn there (Do not draw).
    Clip,
}

impl Edge {
    /// Every edge process, in the order Tool Property offers them.
    pub const ALL: [Self; 4] = [Self::Extend, Self::Repeat, Self::Mirror, Self::Clip];

    /// Its stable name in settings and documents.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Extend => "extend",
            Self::Repeat => "repeat",
            Self::Mirror => "mirror",
            Self::Clip => "clip",
        }
    }

    /// The edge process named `id` ([`Self::id`]).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|edge| edge.id() == id)
    }

    /// Where `t` (0 at the start, 1 at the end, anything outside) falls
    /// on the bar, or `None` where nothing is drawn.
    #[must_use]
    pub fn fold(self, t: f32) -> Option<f32> {
        if !t.is_finite() {
            return None;
        }
        match self {
            Self::Extend => Some(t.clamp(0.0, 1.0)),
            // The end itself shows the end color, not the start's again.
            Self::Repeat if t > 0.0 && t.fract() == 0.0 => Some(1.0),
            Self::Repeat => Some(t.rem_euclid(1.0)),
            Self::Mirror => {
                let k = t.rem_euclid(2.0);
                Some(if k > 1.0 { 2.0 - k } else { k })
            }
            Self::Clip => (0.0..=1.0).contains(&t).then_some(t),
        }
    }
}

/// The narrowest an ellipse gradient gets, against its length.
pub const MIN_ASPECT: f32 = 0.05;

/// A gradient as it is laid: its stops (with their colors filled in), its
/// shape and its edge process.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The stops, and the mixing curve that bends the place first.
    pub map: GradientMap,
    /// How far along a pixel lies.
    pub shape: Shape,
    /// Before the start and past the end.
    pub edge: Edge,
    /// An ellipse's short axis against its long one, [`MIN_ASPECT`] to 1.
    pub aspect: f32,
}

impl Default for Look {
    /// Black to white, straight across, the end colors carrying on.
    fn default() -> Self {
        Self {
            map: GradientMap::default(),
            shape: Shape::Linear,
            edge: Edge::Extend,
            aspect: 0.5,
        }
    }
}

impl Look {
    /// How far along the gradient from `from` to `to` the point `p` lies:
    /// 0 at the start, 1 at the end, outside that before and past them.
    #[must_use]
    pub fn place(&self, [from, to]: [[f32; 2]; 2], p: [f32; 2]) -> f32 {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length_sq = (dx * dx + dy * dy).max(1e-6);
        let (px, py) = (p[0] - from[0], p[1] - from[1]);
        match self.shape {
            Shape::Linear => (px * dx + py * dy) / length_sq,
            Shape::Circle => ((px * px + py * py) / length_sq).sqrt(),
            Shape::Ellipse => {
                let length = length_sq.sqrt();
                // Along the line and across it, each against its radius.
                let along = (px * dx + py * dy) / length;
                let across = (py * dx - px * dy) / length;
                let short = length * self.aspect.clamp(MIN_ASPECT, 1.0);
                ((along / length).powi(2) + (across / short).powi(2)).sqrt()
            }
        }
    }

    /// The color and opacity (straight, 0 to 1) at `p`; clear where the
    /// edge process draws nothing.
    #[must_use]
    pub fn color(&self, line: [[f32; 2]; 2], p: [f32; 2]) -> [f32; 4] {
        self.color_at(self.place(line, p))
    }

    /// The color and opacity at place `t` along the gradient.
    #[must_use]
    pub fn color_at(&self, t: f32) -> [f32; 4] {
        self.edge.fold(t).map_or([0.0; 4], |t| self.map.at(t))
    }

    /// `true` when a stop takes a drawing color, so the look must have
    /// its colors filled in ([`GradientMap::with_drawing`]) before use.
    #[must_use]
    pub fn takes_colors(&self) -> bool {
        self.map
            .stops()
            .iter()
            .any(|stop| stop.color != StopColor::Own)
    }
}

/// How a gradient meets the pixels it is laid on.
#[derive(Clone, Copy, Debug)]
pub struct Lay<'a> {
    /// Only inside this selection (softly where it is soft), or
    /// everywhere.
    pub selection: Option<&'a Selection>,
    /// How much of the gradient goes down, 0 to 1.
    pub opacity: f32,
    /// How its colors mix with the pixels under them.
    pub blend: BlendMode,
    /// The pixels' alpha stays as it is (Lock Transparent Pixels).
    pub alpha_lock: bool,
    /// The gradient clears instead of painting: a pixel loses as much of
    /// its alpha as the gradient is opaque there (the transparent color).
    pub clear: bool,
}

impl Default for Lay<'_> {
    fn default() -> Self {
        Self {
            selection: None,
            opacity: 1.0,
            blend: BlendMode::Normal,
            alpha_lock: false,
            clear: false,
        }
    }
}

/// Lays `look` from `line` (picture pixels) over a layer's straight RGBA8
/// `pixels` of `layer_size`, whose top left sits at `offset` in the
/// picture.
pub fn lay(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    line: [[f32; 2]; 2],
    look: &Look,
    how: &Lay<'_>,
) {
    lay_each(pixels, layer_size, offset, how, |px, py| {
        Some(look.color(line, [px as f32 + 0.5, py as f32 + 0.5]))
    });
}

/// Lays a color at each picture pixel `shade` gives one for (straight
/// RGBA, 0 to 1), as [`Lay`] says.
pub fn lay_each(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    how: &Lay<'_>,
    mut shade: impl FnMut(i64, i64) -> Option<[f32; 4]>,
) {
    if how.clear && how.alpha_lock {
        // Lock Transparent Pixels keeps alpha, so clearing changes
        // nothing (as the bucket's clearing does).
        return;
    }
    let opacity = how.opacity.clamp(0.0, 1.0);
    for y in 0..layer_size[1] {
        for x in 0..layer_size[0] {
            let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
            let cover = match how.selection {
                Some(selection) if px >= 0 && py >= 0 => selection.at(px as u32, py as u32),
                Some(_) => 0.0,
                None => 1.0,
            };
            if cover <= 0.0 {
                continue;
            }
            let Some(mut color) = shade(px, py) else {
                continue;
            };
            color[3] *= cover * opacity;
            if color[3] <= 0.0 {
                continue;
            }
            let at = ((y * layer_size[0] + x) * 4) as usize;
            let Some(pixel) = pixels.get_mut(at..at + 4) else {
                continue;
            };
            let old = crate::doc::unit(pixel);
            let out = if how.clear {
                [old[0], old[1], old[2], old[3] * (1.0 - color[3])]
            } else if how.alpha_lock {
                let mixed = how.blend.composite([old[0], old[1], old[2], 1.0], color);
                [mixed[0], mixed[1], mixed[2], old[3]]
            } else {
                how.blend.composite(old, color)
            };
            pixel.copy_from_slice(&crate::doc::bytes(out));
        }
    }
}

/// Contour line fill: for each pixel of a picture of `size`, how far into
/// the area `mask` covers (at least half) it lies, 0 on the area's
/// outline and 1 at `width` pixels in (or, with `width` 0, at the
/// deepest point of the area); `None` outside the area. Places run on
/// past 1 inward, so an edge process can repeat the gradient in bands.
#[must_use]
pub fn contour_places(mask: &[u8], size: [u32; 2], width: f32) -> Vec<Option<f32>> {
    let [w, h] = [size[0] as usize, size[1] as usize];
    let count = w * h;
    if mask.len() < count || count == 0 {
        return vec![None; count];
    }
    let inside = |index: usize| mask[index] >= 128;
    // Squared distance to the nearest pixel outside the area; the canvas
    // edge counts as outside.
    let far = ((w + h + 2) * (w + h + 2)) as f32;
    let mut grid: Vec<f32> = (0..count)
        .map(|index| if inside(index) { far } else { 0.0 })
        .collect();
    let mut line = Vec::new();
    // Each column, with an outside pixel above and below it.
    for x in 0..w {
        line.clear();
        line.push(0.0);
        line.extend((0..h).map(|y| grid[y * w + x]));
        line.push(0.0);
        let out = distance_1d(&line);
        for y in 0..h {
            grid[y * w + x] = out[y + 1];
        }
    }
    // Then each row, with an outside pixel at its ends.
    for y in 0..h {
        line.clear();
        line.push(0.0);
        line.extend_from_slice(&grid[y * w..y * w + w]);
        line.push(0.0);
        let out = distance_1d(&line);
        grid[y * w..y * w + w].copy_from_slice(&out[1..=w]);
    }
    // A pixel on the outline is the start.
    let depth: Vec<Option<f32>> = (0..count)
        .map(|index| inside(index).then(|| (grid[index].sqrt() - 1.0).max(0.0)))
        .collect();
    let span = if width > 0.0 {
        width
    } else {
        depth
            .iter()
            .flatten()
            .fold(0.0_f32, |deepest, found| deepest.max(*found))
            .max(1.0)
    };
    depth
        .into_iter()
        .map(|found| found.map(|found| found / span))
        .collect()
}

/// The squared distance transform of one line of samples (0 at a source,
/// large elsewhere), by the lower envelope of parabolas (Felzenszwalb and
/// Huttenlocher).
fn distance_1d(values: &[f32]) -> Vec<f32> {
    let n = values.len();
    let mut out = vec![0.0; n];
    if n == 0 {
        return out;
    }
    let mut hull = vec![0_usize; n];
    let mut bounds = vec![0.0_f32; n + 1];
    let mut k = 0;
    bounds[0] = f32::NEG_INFINITY;
    bounds[1] = f32::INFINITY;
    let cross = |q: usize, p: usize| {
        let (q, p) = (q as f32, p as f32);
        ((values[q as usize] + q * q) - (values[p as usize] + p * p)) / (2.0 * q - 2.0 * p)
    };
    for q in 1..n {
        let mut s = cross(q, hull[k]);
        // The first bound is minus infinity, so this stops at the first
        // parabola at the latest.
        while s <= bounds[k] {
            k -= 1;
            s = cross(q, hull[k]);
        }
        k += 1;
        hull[k] = q;
        bounds[k] = s;
        bounds[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, slot) in out.iter_mut().enumerate() {
        while bounds[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - hull[k] as f32;
        *slot = d * d + values[hull[k]];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gradient_map::Stop;

    fn black_to_white() -> Look {
        Look {
            map: GradientMap::two([0; 3], [255; 3]),
            ..Look::default()
        }
    }

    #[test]
    fn names_round_trip() {
        for shape in Shape::ALL {
            assert_eq!(Shape::from_id(shape.id()), Some(shape));
        }
        for edge in Edge::ALL {
            assert_eq!(Edge::from_id(edge.id()), Some(edge));
        }
        assert_eq!(Shape::from_id("star"), None);
        assert_eq!(Edge::from_id(""), None);
    }

    #[test]
    fn each_edge_process_folds_places_outside_the_bar() {
        assert_eq!(Edge::Extend.fold(-0.5), Some(0.0));
        assert_eq!(Edge::Extend.fold(1.5), Some(1.0));
        assert_eq!(Edge::Repeat.fold(1.25), Some(0.25));
        assert_eq!(Edge::Repeat.fold(1.0), Some(1.0), "the end keeps its color");
        assert_eq!(Edge::Repeat.fold(-0.25), Some(0.75));
        assert_eq!(Edge::Mirror.fold(1.25), Some(0.75));
        assert_eq!(Edge::Mirror.fold(2.25), Some(0.25));
        assert_eq!(Edge::Clip.fold(1.01), None);
        assert_eq!(Edge::Clip.fold(-0.01), None);
        assert_eq!(Edge::Clip.fold(0.5), Some(0.5));
        assert_eq!(Edge::Extend.fold(f32::NAN), None);
    }

    #[test]
    fn shapes_measure_from_the_start() {
        let line = [[10.0, 10.0], [20.0, 10.0]];
        let mut look = black_to_white();
        assert!(
            (look.place(line, [15.0, 99.0]) - 0.5).abs() < 1e-5,
            "across"
        );
        look.shape = Shape::Circle;
        assert!((look.place(line, [10.0, 15.0]) - 0.5).abs() < 1e-5);
        assert!((look.place(line, [5.0, 10.0]) - 0.5).abs() < 1e-5);
        look.shape = Shape::Ellipse;
        look.aspect = 0.5;
        // Along the line the radius is 10, across it 5.
        assert!((look.place(line, [20.0, 10.0]) - 1.0).abs() < 1e-5);
        assert!((look.place(line, [10.0, 15.0]) - 1.0).abs() < 1e-5);
        assert!((look.place(line, [10.0, 12.5]) - 0.5).abs() < 1e-5);
        // A turned line turns the ellipse with it.
        let up = [[10.0, 10.0], [10.0, 0.0]];
        assert!((look.place(up, [15.0, 10.0]) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn laying_blends_clears_and_keeps_inside_the_selection() {
        let size = [4_u32, 1];
        let line = [[0.0, 0.0], [4.0, 0.0]];
        let look = Look {
            map: GradientMap::of(&[Stop::new(0.0, [255, 0, 0]), Stop::new(1.0, [255, 0, 0])])
                .unwrap(),
            ..Look::default()
        };
        // Normal over gray.
        let mut pixels = [128_u8, 128, 128, 255].repeat(4);
        lay(&mut pixels, size, [0, 0], line, &look, &Lay::default());
        assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
        // Multiply over gray keeps the red channel only.
        let mut pixels = [128_u8, 128, 128, 255].repeat(4);
        let multiply = Lay {
            blend: BlendMode::Multiply,
            ..Lay::default()
        };
        lay(&mut pixels, size, [0, 0], line, &look, &multiply);
        assert_eq!(&pixels[..4], &[128, 0, 0, 255]);
        // Clearing takes the alpha, a locked layer keeps it.
        let clear = Lay {
            clear: true,
            ..Lay::default()
        };
        let mut pixels = [9_u8, 9, 9, 255].repeat(4);
        lay(&mut pixels, size, [0, 0], line, &look, &clear);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0));
        let mut pixels = [9_u8, 9, 9, 255].repeat(4);
        lay(
            &mut pixels,
            size,
            [0, 0],
            line,
            &look,
            &Lay {
                alpha_lock: true,
                ..clear
            },
        );
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        // Only the selected half.
        let mut selection = Selection::new(4, 1);
        selection.rect([0, 0, 2, 1], crate::select::Op::Replace);
        let mut pixels = vec![0_u8; 16];
        lay(
            &mut pixels,
            size,
            [0, 0],
            line,
            &look,
            &Lay {
                selection: Some(&selection),
                ..Lay::default()
            },
        );
        assert_eq!(pixels[3], 255);
        assert_eq!(pixels[15], 0);
        // Do not draw leaves what lies past the end.
        let mut pixels = vec![0_u8; 16];
        let short = [[0.0, 0.0], [2.0, 0.0]];
        let clip = Look {
            edge: Edge::Clip,
            ..look
        };
        lay(&mut pixels, size, [0, 0], short, &clip, &Lay::default());
        assert_eq!(pixels[3], 255);
        assert_eq!(pixels[15], 0, "past the end nothing is drawn");
    }

    #[test]
    fn contour_places_run_from_the_outline_inward() {
        // A 9 by 9 square on a 11 by 11 picture.
        let size = [11_u32, 11];
        let mask: Vec<u8> = (0..121)
            .map(|index| {
                let (x, y) = (index % 11, index / 11);
                if (1..10).contains(&x) && (1..10).contains(&y) {
                    255
                } else {
                    0
                }
            })
            .collect();
        let places = contour_places(&mask, size, 0.0);
        assert_eq!(places[0], None, "outside");
        let edge = places[11 + 1].unwrap();
        let middle = places[5 * 11 + 5].unwrap();
        assert!(edge.abs() < 1e-5, "the outline is the start: {edge}");
        assert!((middle - 1.0).abs() < 1e-5, "the deepest is the end");
        let between = places[5 * 11 + 3].unwrap();
        assert!(between > 0.0 && between < 1.0, "{between}");
        // A width of 2: the middle lies twice past the end.
        let banded = contour_places(&mask, size, 2.0);
        assert!((banded[5 * 11 + 5].unwrap() - 2.0).abs() < 1e-5);
        // The canvas edge counts as outline.
        let full = contour_places(&[255; 9], [3, 3], 0.0);
        assert!(full[0].unwrap().abs() < 1e-5);
        assert!((full[4].unwrap() - 1.0).abs() < 1e-5);
        assert!(contour_places(&[], [0, 0], 0.0).is_empty());
    }
}
