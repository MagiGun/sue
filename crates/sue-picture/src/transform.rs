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

/// `rest` (the layer without the piece, `width` by `height`) with `piece`
/// (`size` pixels) laid over it on `quad`, straight RGBA8.
#[must_use]
pub fn place(
    rest: &[u8],
    width: u32,
    height: u32,
    piece: &[u8],
    size: [u32; 2],
    quad: &Quad,
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
            let source = crate::warp::sample(
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
}
