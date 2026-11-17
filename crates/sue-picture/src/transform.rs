//! Transforming a layer or the selected part of it, as Clip Studio Paint's
//! Scale/Rotate and Free Transform do. The piece is lifted off the layer
//! and drawn onto a quadrilateral: a rotated, scaled rectangle for
//! Scale/Rotate, any four corners for Free Transform. Both are one
//! projective mapping (a homography) from the piece's rectangle to the
//! quad, worked backwards so every pixel of the result samples the piece
//! with premultiplied bilinear filtering.

use crate::select::Selection;

/// A quad's corners: top left, top right, bottom right, bottom left.
pub type Quad = [[f32; 2]; 4];

/// A 3 by 3 matrix, row by row.
type Matrix = [f64; 9];

/// The homography from the unit square to `quad` (Heckbert's square to
/// quad): corner 0 at (0, 0), 1 at (1, 0), 2 at (1, 1), 3 at (0, 1).
#[must_use]
fn square_to_quad(quad: &Quad) -> Matrix {
    let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = quad.map(|[x, y]| [f64::from(x), f64::from(y)]);
    let (sx, sy) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
    if sx.abs() < 1e-12 && sy.abs() < 1e-12 {
        // A parallelogram: affine.
        return [x1 - x0, x2 - x1, x0, y1 - y0, y2 - y1, y0, 0.0, 0.0, 1.0];
    }
    let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
    let det = dx1 * dy2 - dx2 * dy1;
    let g = (sx * dy2 - dx2 * sy) / det;
    let h = (dx1 * sy - sx * dy1) / det;
    [
        x1 - x0 + g * x1,
        x3 - x0 + h * x3,
        x0,
        y1 - y0 + g * y1,
        y3 - y0 + h * y3,
        y0,
        g,
        h,
        1.0,
    ]
}

/// The inverse of `m`, or `None` if it squashes everything flat.
fn invert(m: &Matrix) -> Option<Matrix> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    Some(
        [
            e * i - f * h,
            c * h - b * i,
            b * f - c * e,
            f * g - d * i,
            a * i - c * g,
            c * d - a * f,
            d * h - e * g,
            b * g - a * h,
            a * e - b * d,
        ]
        .map(|value| value / det),
    )
}

/// `p` through `m`.
fn apply(m: &Matrix, [x, y]: [f64; 2]) -> [f64; 2] {
    let w = m[6] * x + m[7] * y + m[8];
    [
        (m[0] * x + m[1] * y + m[2]) / w,
        (m[3] * x + m[4] * y + m[5]) / w,
    ]
}

/// Where the point `uv` of the unit square lands on `quad`.
#[must_use]
pub fn on_quad(quad: &Quad, [u, v]: [f32; 2]) -> [f32; 2] {
    let [x, y] = apply(&square_to_quad(quad), [f64::from(u), f64::from(v)]);
    [x as f32, y as f32]
}

/// The corners of a `width` by `height` rectangle scaled by `scale`,
/// turned `angle` radians and centered on `center`.
#[must_use]
pub fn rotated(center: [f32; 2], width: f32, height: f32, scale: [f32; 2], angle: f32) -> Quad {
    let (sin, cos) = angle.sin_cos();
    let (hw, hh) = (width * scale[0] / 2.0, height * scale[1] / 2.0);
    [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
        .map(|[x, y]| [center[0] + x * cos - y * sin, center[1] + x * sin + y * cos])
}

/// What dragging a corner of the transform box does (Clip Studio Paint's
/// transform modes, in Tool Property's order).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    /// A corner scales, keeping the shape; outside turns.
    #[default]
    ScaleRotate,
    /// A corner scales; nothing turns.
    Scale,
    /// Every drag turns it.
    Rotate,
    /// Each corner goes where it is dragged.
    Free,
    /// A corner slides its edge along itself.
    Skew,
    /// Each corner goes where it is dragged (as Free, kept apart for the
    /// menu).
    Distort,
    /// A corner and the one across its edge move apart or together.
    Perspective,
}

impl Mode {
    /// Every mode, in Tool Property's order.
    pub const ALL: [Self; 7] = [
        Self::ScaleRotate,
        Self::Scale,
        Self::Rotate,
        Self::Free,
        Self::Skew,
        Self::Distort,
        Self::Perspective,
    ];

    /// Its stable name in saved settings.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ScaleRotate => "scale_rotate",
            Self::Scale => "scale",
            Self::Rotate => "rotate",
            Self::Free => "free",
            Self::Skew => "skew",
            Self::Distort => "distort",
            Self::Perspective => "perspective",
        }
    }

    /// The mode named `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.id() == id)
    }

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::ScaleRotate => tr!("scale-rotate"),
            Self::Scale => tr!("scale"),
            Self::Rotate => tr!("rotate"),
            Self::Free => tr!("free-transform"),
            Self::Skew => tr!("skew"),
            Self::Distort => tr!("distort"),
            Self::Perspective => tr!("perspective"),
        }
    }

    /// `true` if its corners move one by one (the box stops being a
    /// rotated rectangle).
    #[must_use]
    pub const fn frees_corners(self) -> bool {
        matches!(
            self,
            Self::Free | Self::Skew | Self::Distort | Self::Perspective
        )
    }
}

/// The middle of `quad`: where its rotation centre starts.
#[must_use]
pub fn middle(quad: &Quad) -> [f32; 2] {
    let sum = quad.iter().fold([0.0, 0.0], |sum, corner| {
        [sum[0] + corner[0], sum[1] + corner[1]]
    });
    [sum[0] / 4.0, sum[1] / 4.0]
}

/// `quad` turned `angle` radians (clockwise on the screen) around
/// `pivot`, the rotation centre, which can be anywhere.
#[must_use]
pub fn turned(quad: &Quad, pivot: [f32; 2], angle: f32) -> Quad {
    let (sin, cos) = angle.sin_cos();
    quad.map(|[x, y]| {
        let (dx, dy) = (x - pivot[0], y - pivot[1]);
        [
            pivot[0] + dx * cos - dy * sin,
            pivot[1] + dx * sin + dy * cos,
        ]
    })
}

/// The angle from `pivot` to `from` turned to the angle from `pivot` to
/// `to`, snapped to steps of `step` radians when `step` is above nothing
/// (Snap angle, or Shift).
#[must_use]
pub fn drag_angle(pivot: [f32; 2], from: [f32; 2], to: [f32; 2], step: f32) -> f32 {
    let a = (from[1] - pivot[1]).atan2(from[0] - pivot[0]);
    let b = (to[1] - pivot[1]).atan2(to[0] - pivot[0]);
    let angle = b - a;
    if step > 0.0 {
        (angle / step).round() * step
    } else {
        angle
    }
}

/// `quad` with corner `corner` (0 top left, then clockwise) dragged to
/// `to` in `mode`, scaling about `pivot` where the mode scales. `keep`
/// keeps the shape where the mode would let it change (Shift in Scale).
#[must_use]
pub fn drag_corner(
    quad: &Quad,
    mode: Mode,
    corner: usize,
    to: [f32; 2],
    pivot: [f32; 2],
    keep: bool,
) -> Quad {
    let corner = corner % 4;
    let mut out = *quad;
    let from = quad[corner];
    match mode {
        Mode::Free | Mode::Distort => out[corner] = to,
        Mode::Rotate => {
            let angle = drag_angle(pivot, from, to, 0.0);
            out = turned(quad, pivot, angle);
        }
        Mode::ScaleRotate | Mode::Scale => {
            // Scale along the box's own sides, about the pivot.
            let across = sub(quad[1], quad[0]);
            let down = sub(quad[3], quad[0]);
            let (ua, ud) = (unit(across), unit(down));
            let (before, after) = (sub(from, pivot), sub(to, pivot));
            let ratio = |axis: [f32; 2]| {
                let was = dot(before, axis);
                if was.abs() < 1e-4 {
                    1.0
                } else {
                    dot(after, axis) / was
                }
            };
            let (mut sx, mut sy) = (ratio(ua), ratio(ud));
            if keep {
                let both = if sx.abs() > sy.abs() { sx } else { sy };
                (sx, sy) = (both, both);
            }
            out = quad.map(|point| {
                let d = sub(point, pivot);
                let (along, downward) = (dot(d, ua) * sx, dot(d, ud) * sy);
                [
                    pivot[0] + ua[0] * along + ud[0] * downward,
                    pivot[1] + ua[1] * along + ud[1] * downward,
                ]
            });
        }
        Mode::Skew => {
            // The corner and its neighbour along the top or bottom edge
            // slide together along that edge.
            let partner = [1, 0, 3, 2][corner];
            let edge = unit(sub(quad[partner], quad[corner]));
            let along = dot(sub(to, from), edge);
            for at in [corner, partner] {
                out[at] = [quad[at][0] + edge[0] * along, quad[at][1] + edge[1] * along];
            }
        }
        Mode::Perspective => {
            // The corner goes along its edge; the one across the edge goes
            // the other way, as far.
            let partner = [1, 0, 3, 2][corner];
            let edge = unit(sub(quad[partner], quad[corner]));
            let along = dot(sub(to, from), edge);
            out[corner] = [from[0] + edge[0] * along, from[1] + edge[1] * along];
            out[partner] = [
                quad[partner][0] - edge[0] * along,
                quad[partner][1] - edge[1] * along,
            ];
        }
    }
    out
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit(a: [f32; 2]) -> [f32; 2] {
    let length = a[0].hypot(a[1]);
    if length < 1e-6 {
        [1.0, 0.0]
    } else {
        [a[0] / length, a[1] / length]
    }
}

/// Mesh Transformation's lattice: a grid of points over the piece, a
/// chosen number across and down, each dragged on its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Lattice {
    /// Points across, at least 2.
    pub columns: usize,
    /// Points down, at least 2.
    pub rows: usize,
    /// Where each point started, row by row.
    pub from: Vec<[f32; 2]>,
    /// Where each point is now.
    pub to: Vec<[f32; 2]>,
}

impl Lattice {
    /// The most points either way.
    pub const MOST: usize = 33;

    /// A `columns` by `rows` lattice of points (clamped to 2 to
    /// [`Self::MOST`]) spread evenly over `[x0, y0, x1, y1]`.
    #[must_use]
    pub fn over([x0, y0, x1, y1]: [f32; 4], columns: usize, rows: usize) -> Self {
        let (columns, rows) = (columns.clamp(2, Self::MOST), rows.clamp(2, Self::MOST));
        let from: Vec<[f32; 2]> = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| {
                [
                    x0 + (x1 - x0) * column as f32 / (columns - 1) as f32,
                    y0 + (y1 - y0) * row as f32 / (rows - 1) as f32,
                ]
            })
            .collect();
        Self {
            columns,
            rows,
            to: from.clone(),
            from,
        }
    }

    /// Moves point `(column, row)` to `to`.
    pub fn drag(&mut self, column: usize, row: usize, to: [f32; 2]) -> bool {
        if column >= self.columns || row >= self.rows {
            return false;
        }
        self.to[row * self.columns + column] = to;
        true
    }

    /// `source` (RGBA8, `width` by `height`) bent by the lattice.
    #[must_use]
    pub fn apply(&self, source: &[u8], width: u32, height: u32) -> Vec<u8> {
        if self.from == self.to {
            return source.to_vec();
        }
        crate::warp::lattice(
            source,
            width,
            height,
            [self.columns, self.rows],
            &self.from,
            &self.to,
            crate::warp::Interpolation::Smooth,
        )
    }
}

/// The piece to transform: the part of a layer (RGBA8) under `selection`
/// (in the layer's pixels), or all of its pixels. Returns the piece's
/// rectangle `[x0, y0, x1, y1]`, its pixels and the layer without it.
#[must_use]
pub fn lift(
    pixels: &[u8],
    width: u32,
    height: u32,
    selection: Option<&Selection>,
) -> Option<([u32; 4], Vec<u8>, Vec<u8>)> {
    let cover = |x: u32, y: u32| selection.map_or(1.0, |selection| selection.at(x, y));
    let mut bounds: Option<[u32; 4]> = None;
    for y in 0..height {
        for x in 0..width {
            if pixels[((y * width + x) * 4 + 3) as usize] == 0 || cover(x, y) <= 0.0 {
                continue;
            }
            bounds = Some(match bounds {
                None => [x, y, x + 1, y + 1],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)],
            });
        }
    }
    let [x0, y0, x1, y1] = bounds?;
    let (w, h) = (x1 - x0, y1 - y0);
    let mut piece = vec![0; (w * h * 4) as usize];
    let mut rest = pixels.to_vec();
    for y in y0..y1 {
        for x in x0..x1 {
            let at = ((y * width + x) * 4) as usize;
            let to = (((y - y0) * w + (x - x0)) * 4) as usize;
            let share = cover(x, y);
            piece[to..to + 4].copy_from_slice(&pixels[at..at + 4]);
            piece[to + 3] = (f32::from(pixels[at + 3]) * share).round() as u8;
            rest[at + 3] = (f32::from(pixels[at + 3]) * (1.0 - share)).round() as u8;
        }
    }
    Some(([x0, y0, x1, y1], piece, rest))
}

/// How a transform samples the piece it moves (Interpolation method):
/// each pixel of the result reads the piece between its pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Interpolation {
    /// The nearest pixel: hard edges, every pixel whole, for pixel art
    /// and line art that must stay crisp.
    Nearest,
    /// Between the four nearest pixels (bilinear): smooth.
    #[default]
    Smooth,
    /// Between the sixteen nearest pixels (bicubic): smooth and sharper
    /// where a picture is scaled up.
    Bicubic,
}

impl Interpolation {
    /// Every choice, in Tool Property's order.
    pub const ALL: [Self; 3] = [Self::Nearest, Self::Smooth, Self::Bicubic];

    /// Its stable name in saved settings.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Smooth => "smooth",
            Self::Bicubic => "bicubic",
        }
    }

    /// The choice saved as `id`, if it is one.
    #[must_use]
    pub fn of_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.id() == id)
    }

    /// The pixel of `source` (`width` by `height`, straight RGBA8) at
    /// `x`, `y` (pixel centres at halves), transparent outside it.
    #[must_use]
    pub fn sample(self, source: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
        match self {
            Self::Smooth => crate::warp::sample(source, width, height, x, y),
            Self::Nearest => {
                let (nx, ny) = (x.floor(), y.floor());
                if nx < 0.0 || ny < 0.0 || nx >= width as f32 || ny >= height as f32 {
                    return [0; 4];
                }
                let at = ((ny as u32 * width + nx as u32) * 4) as usize;
                [source[at], source[at + 1], source[at + 2], source[at + 3]]
            }
            Self::Bicubic => bicubic(source, width, height, x, y),
        }
    }
}

/// Catmull-Rom's weight for a pixel `t` away (0 to 2).
fn cubic_weight(t: f32) -> f32 {
    let t = t.abs();
    if t < 1.0 {
        1.5 * t * t * t - 2.5 * t * t + 1.0
    } else if t < 2.0 {
        -0.5 * t * t * t + 2.5 * t * t - 4.0 * t + 2.0
    } else {
        0.0
    }
}

/// Bicubic (Catmull-Rom) sampling over premultiplied colors, so a clear
/// pixel's color never bleeds in; outside the source is clear.
fn bicubic(source: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let mut sum = [0.0_f32; 4];
    for dy in -1..=2_i64 {
        let wy = cubic_weight(fy - dy as f32);
        let py = y0 as i64 + dy;
        if wy == 0.0 || py < 0 || py >= i64::from(height) {
            continue;
        }
        for dx in -1..=2_i64 {
            let wx = cubic_weight(fx - dx as f32);
            let px = x0 as i64 + dx;
            if wx == 0.0 || px < 0 || px >= i64::from(width) {
                continue;
            }
            let at = ((py as u32 * width + px as u32) * 4) as usize;
            let weight = wx * wy;
            let a = f32::from(source[at + 3]) / 255.0;
            for channel in 0..3 {
                sum[channel] += f32::from(source[at + channel]) * a * weight;
            }
            sum[3] += a * weight;
        }
    }
    let alpha = sum[3].clamp(0.0, 1.0);
    if alpha <= 1e-6 {
        return [0; 4];
    }
    [
        (sum[0] / alpha).round().clamp(0.0, 255.0) as u8,
        (sum[1] / alpha).round().clamp(0.0, 255.0) as u8,
        (sum[2] / alpha).round().clamp(0.0, 255.0) as u8,
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// `rest` (the layer without the piece, `width` by `height`) with `piece`
/// (`size` pixels) laid over it on `quad`, straight RGBA8, sampled
/// smoothly.
#[must_use]
pub fn place(
    rest: &[u8],
    width: u32,
    height: u32,
    piece: &[u8],
    size: [u32; 2],
    quad: &Quad,
) -> Vec<u8> {
    place_with(
        rest,
        width,
        height,
        piece,
        size,
        quad,
        Interpolation::Smooth,
    )
}

/// [`place`], sampling the piece as `interpolation` says.
#[must_use]
#[allow(clippy::too_many_arguments, reason = "place and how it samples")]
pub fn place_with(
    rest: &[u8],
    width: u32,
    height: u32,
    piece: &[u8],
    size: [u32; 2],
    quad: &Quad,
    interpolation: Interpolation,
) -> Vec<u8> {
    let mut out = rest.to_vec();
    let Some(back) = invert(&square_to_quad(quad)) else {
        return out;
    };
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for [x, y] in quad {
        (x0, y0, x1, y1) = (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y));
    }
    let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
    for y in clamp(y0.floor(), height)..clamp(y1.ceil() + 1.0, height) {
        for x in clamp(x0.floor(), width)..clamp(x1.ceil() + 1.0, width) {
            let [u, v] = apply(&back, [f64::from(x) + 0.5, f64::from(y) + 0.5]);
            if !(-0.01..=1.01).contains(&u) || !(-0.01..=1.01).contains(&v) {
                continue;
            }
            let source = interpolation.sample(
                piece,
                size[0],
                size[1],
                (u * f64::from(size[0])) as f32,
                (v * f64::from(size[1])) as f32,
            );
            let a = f32::from(source[3]) / 255.0;
            if a <= 0.0 {
                continue;
            }
            let at = ((y * width + x) * 4) as usize;
            let under = f32::from(out[at + 3]) / 255.0;
            let alpha = a + under * (1.0 - a);
            for channel in 0..3 {
                let mixed = f32::from(source[channel]) * a
                    + f32::from(out[at + channel]) * under * (1.0 - a);
                out[at + channel] = (mixed / alpha).round().clamp(0.0, 255.0) as u8;
            }
            out[at + 3] = (alpha * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn quads_map_the_square_corner_to_corner() {
        let quad: Quad = [[10.0, 10.0], [50.0, 0.0], [60.0, 60.0], [0.0, 40.0]];
        for (corner, uv) in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            .iter()
            .enumerate()
        {
            assert!(close(on_quad(&quad, *uv), quad[corner]), "corner {corner}");
        }
        // A turned rectangle is a parallelogram, mapped affinely.
        let turned = rotated(
            [50.0, 50.0],
            40.0,
            20.0,
            [1.0, 1.0],
            std::f32::consts::FRAC_PI_2,
        );
        assert!(close(turned[0], [60.0, 30.0]));
        assert!(close(on_quad(&turned, [0.5, 0.5]), [50.0, 50.0]));
    }

    #[test]
    fn a_lifted_piece_scales_into_its_new_place() {
        let (width, height) = (40, 40);
        let mut pixels = vec![0; (width * height * 4) as usize];
        for y in 10..20 {
            for x in 10..20 {
                pixels[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[200, 30, 30, 255]);
            }
        }
        let (rect, piece, rest) = lift(&pixels, width, height, None).unwrap();
        assert_eq!(rect, [10, 10, 20, 20]);
        assert!(
            rest.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0),
            "lifted off"
        );
        // Twice as big, from 5 to 25.
        let quad = rotated([15.0, 15.0], 10.0, 10.0, [2.0, 2.0], 0.0);
        let out = place(&rest, width, height, &piece, [10, 10], &quad);
        let alpha = |x: u32, y: u32| out[((y * width + x) * 4 + 3) as usize];
        assert_eq!(alpha(6, 6), 255);
        assert_eq!(alpha(23, 23), 255);
        assert_eq!(alpha(3, 3), 0);
        assert_eq!(
            &out[((15 * width + 15) * 4) as usize..][..3],
            &[200, 30, 30]
        );
        // With a selection only the selected part lifts.
        let mut selection = Selection::new(width, height);
        selection.rect([10, 10, 15, 20], crate::select::Op::Replace);
        let (rect, _, rest) = lift(&pixels, width, height, Some(&selection)).unwrap();
        assert_eq!(rect, [10, 10, 15, 20]);
        assert_eq!(
            rest[((12 * width + 17) * 4 + 3) as usize],
            255,
            "the rest stays"
        );
        assert!(lift(&[0; 64], 4, 4, None).is_none(), "nothing to lift");
    }

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    const BOX: Quad = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];

    #[test]
    fn transform_modes_are_data_with_stable_names() {
        assert_eq!(Mode::ALL.len(), 7);
        for mode in Mode::ALL {
            assert_eq!(Mode::from_id(mode.id()), Some(mode));
            assert!(!mode.label().is_empty());
        }
        assert!(Mode::Free.frees_corners() && !Mode::ScaleRotate.frees_corners());
    }

    #[test]
    fn each_mode_drags_a_corner_its_own_way() {
        let pivot = middle(&BOX);
        assert_eq!(pivot, [5.0, 5.0]);
        // Free and Distort: that corner alone.
        let free = drag_corner(&BOX, Mode::Free, 2, [14.0, 12.0], pivot, false);
        assert_eq!(free[2], [14.0, 12.0]);
        assert_eq!(&free[..2], &BOX[..2]);
        // Scale about the far corner, freely or keeping the shape.
        let scaled = drag_corner(&BOX, Mode::Scale, 2, [20.0, 15.0], BOX[0], false);
        assert!(near(scaled[2], [20.0, 15.0]) && near(scaled[0], [0.0, 0.0]));
        let kept = drag_corner(&BOX, Mode::Scale, 2, [20.0, 15.0], BOX[0], true);
        assert!(near(kept[2], [20.0, 20.0]), "{kept:?}");
        // Rotate: a quarter turn of a corner turns the box about the pivot.
        let turned_box = drag_corner(&BOX, Mode::Rotate, 1, [10.0, 10.0], pivot, false);
        assert!(near(turned_box[0], [10.0, 0.0]), "{turned_box:?}");
        // Skew: the top edge slides along itself.
        let skewed = drag_corner(&BOX, Mode::Skew, 0, [3.0, 4.0], pivot, false);
        assert!(near(skewed[0], [3.0, 0.0]) && near(skewed[1], [13.0, 0.0]));
        assert_eq!(&skewed[2..], &BOX[2..]);
        // Perspective: the corner and the one across its edge mirror.
        let perspective = drag_corner(&BOX, Mode::Perspective, 0, [2.0, 0.0], pivot, false);
        assert!(near(perspective[0], [2.0, 0.0]) && near(perspective[1], [8.0, 0.0]));
    }

    #[test]
    fn the_rotation_centre_can_be_moved() {
        // Turned about its middle, the middle stays; about a corner, the
        // corner stays and the middle goes round it.
        let about_middle = turned(&BOX, [5.0, 5.0], std::f32::consts::FRAC_PI_2);
        assert!(near(middle(&about_middle), [5.0, 5.0]));
        let about_corner = turned(&BOX, [0.0, 0.0], std::f32::consts::FRAC_PI_2);
        assert!(near(about_corner[0], [0.0, 0.0]));
        assert!(near(middle(&about_corner), [-5.0, 5.0]));
        // A drag snapped to 45 degree steps.
        let step = std::f32::consts::FRAC_PI_4;
        let angle = drag_angle([0.0, 0.0], [10.0, 0.0], [10.0, 6.0], step);
        assert!((angle - step).abs() < 1e-5);
        let free = drag_angle([0.0, 0.0], [10.0, 0.0], [10.0, 6.0], 0.0);
        assert!((free - 0.540_42).abs() < 1e-4);
    }

    #[test]
    fn a_lattice_of_any_grid_bends_the_piece() {
        // An 8 by 8 square in a 32 by 32 picture.
        let (width, height) = (32_u32, 32_u32);
        let mut source = vec![0_u8; (width * height * 4) as usize];
        for y in 8..16 {
            for x in 8..16 {
                source[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let mut lattice = Lattice::over([0.0, 0.0, 32.0, 32.0], 5, 3);
        assert_eq!((lattice.columns, lattice.rows), (5, 3));
        assert_eq!(lattice.from.len(), 15);
        assert_eq!(lattice.from[4], [32.0, 0.0]);
        assert_eq!(lattice.apply(&source, width, height), source, "untouched");
        assert_eq!(
            Lattice::over([0.0, 0.0, 1.0, 1.0], 1, 99).columns,
            2,
            "clamped"
        );
        // Every point moved right by 8: the whole piece moves.
        for row in 0..3 {
            for column in 0..5 {
                let [x, y] = lattice.from[row * 5 + column];
                assert!(lattice.drag(column, row, [x + 8.0, y]));
            }
        }
        assert!(!lattice.drag(5, 0, [0.0, 0.0]));
        let moved = lattice.apply(&source, width, height);
        let alpha = |pixels: &[u8], x: u32, y: u32| pixels[((y * width + x) * 4 + 3) as usize];
        assert_eq!(alpha(&moved, 20, 12), 255);
        assert_eq!(alpha(&moved, 10, 12), 0);
        // One middle point dragged bends the piece near it only.
        let mut bend = Lattice::over([0.0, 0.0, 32.0, 32.0], 5, 3);
        bend.drag(1, 1, [8.0, 22.0]);
        let bent = bend.apply(&source, width, height);
        assert_ne!(bent, source);
        assert_eq!(alpha(&source, 12, 17), 0);
        assert_eq!(alpha(&bent, 12, 17), 255, "the bottom edge came down");
        assert_eq!(alpha(&bent, 12, 10), 255, "the top stays");
    }

    #[test]
    fn each_interpolation_samples_its_own_way() {
        // A 2 by 2 checker of black and white, scaled up eight times.
        let mut piece = vec![0_u8; 16];
        for (index, value) in [0_u8, 255, 255, 0].into_iter().enumerate() {
            piece[index * 4..index * 4 + 4].copy_from_slice(&[value, value, value, 255]);
        }
        let (width, height) = (16, 16);
        let rest = vec![0_u8; (width * height * 4) as usize];
        let quad = rotated([8.0, 8.0], 2.0, 2.0, [8.0, 8.0], 0.0);
        let shades = |interpolation: Interpolation| {
            let out = place_with(&rest, width, height, &piece, [2, 2], &quad, interpolation);
            let mut seen: Vec<u8> = (0..width * height)
                .map(|index| out[(index * 4) as usize])
                .collect();
            seen.sort_unstable();
            seen.dedup();
            seen
        };
        // Hard edges: only the two colors there were.
        assert_eq!(shades(Interpolation::Nearest), vec![0, 255]);
        // Smooth and bicubic blend between them.
        assert!(shades(Interpolation::Smooth).len() > 4);
        assert!(shades(Interpolation::Bicubic).len() > 4);
        // Bicubic keeps more contrast near a pixel's middle than bilinear.
        let at = |interpolation: Interpolation| {
            let out = place_with(&rest, width, height, &piece, [2, 2], &quad, interpolation);
            out[((5 * width + 5) * 4) as usize]
        };
        assert!(at(Interpolation::Bicubic) <= at(Interpolation::Smooth));
        for choice in Interpolation::ALL {
            assert_eq!(Interpolation::of_id(choice.id()), Some(choice));
        }
    }
}
