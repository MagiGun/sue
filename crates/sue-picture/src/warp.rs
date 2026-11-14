//! Puppet warp: pins on a layer bend its pixels around them while keeping
//! shapes as rigid as they can be, as Clip Studio Paint's Puppet warp does.
//! Pins that stay put hold their part of the picture still.
//!
//! The bend is the rigid kind of moving least squares (Schaefer, McPhail
//! and Warren, "Image Deformation Using Moving Least Squares", 2006): every
//! point moves by the rotation and translation that best carries the pins
//! near it, nearer pins counting more. Pixels are found backwards, from
//! where they land to where they came from, on a coarse grid that is
//! filled in between, and sampled with premultiplied bilinear filtering.

/// A pin: where it was put and where it has been dragged (layer pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pin {
    /// Where it was put.
    pub from: [f32; 2],
    /// Where it is now.
    pub to: [f32; 2],
}

impl Pin {
    /// A pin put at `at`.
    #[must_use]
    pub const fn at(at: [f32; 2]) -> Self {
        Self { from: at, to: at }
    }
}

/// Where `v` goes under the rigid moving least squares bend that carries
/// each pair's first point to its second.
#[must_use]
pub fn deform(v: [f32; 2], pairs: &[([f32; 2], [f32; 2])]) -> [f32; 2] {
    match pairs {
        [] => v,
        [(p, q)] => [v[0] + q[0] - p[0], v[1] + q[1] - p[1]],
        _ => {
            let v = [f64::from(v[0]), f64::from(v[1])];
            let wide = |point: [f32; 2]| [f64::from(point[0]), f64::from(point[1])];
            let mut total = 0.0;
            let (mut p_star, mut q_star) = ([0.0; 2], [0.0; 2]);
            for (p, q) in pairs {
                let (p, q) = (wide(*p), wide(*q));
                let d2 = (p[0] - v[0]).powi(2) + (p[1] - v[1]).powi(2);
                if d2 < 1e-10 {
                    return [q[0] as f32, q[1] as f32];
                }
                let w = 1.0 / d2;
                total += w;
                for axis in 0..2 {
                    p_star[axis] += w * p[axis];
                    q_star[axis] += w * q[axis];
                }
            }
            for axis in 0..2 {
                p_star[axis] /= total;
                q_star[axis] /= total;
            }
            let d = [v[0] - p_star[0], v[1] - p_star[1]];
            // With (x, y) perpendicular as (-y, x).
            let d_perp = [-d[1], d[0]];
            let mut f = [0.0; 2];
            for (p, q) in pairs {
                let (p, q) = (wide(*p), wide(*q));
                let w = 1.0 / ((p[0] - v[0]).powi(2) + (p[1] - v[1]).powi(2));
                let ph = [p[0] - p_star[0], p[1] - p_star[1]];
                let qh = [q[0] - q_star[0], q[1] - q_star[1]];
                let ph_perp = [-ph[1], ph[0]];
                let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
                let a00 = dot(ph, d);
                let a01 = -dot(ph, d_perp);
                let a10 = -dot(ph_perp, d);
                let a11 = dot(ph_perp, d_perp);
                f[0] += w * (qh[0] * a00 + qh[1] * a10);
                f[1] += w * (qh[0] * a01 + qh[1] * a11);
            }
            let length = f[0].hypot(f[1]);
            let reach = d[0].hypot(d[1]);
            if length < 1e-12 {
                return [(q_star[0] + d[0]) as f32, (q_star[1] + d[1]) as f32];
            }
            [
                (q_star[0] + f[0] / length * reach) as f32,
                (q_star[1] + f[1] / length * reach) as f32,
            ]
        }
    }
}

/// Where a point of the picture lands once the pins have moved.
#[must_use]
pub fn forward(v: [f32; 2], pins: &[Pin]) -> [f32; 2] {
    let pairs: Vec<_> = pins.iter().map(|pin| (pin.from, pin.to)).collect();
    deform(v, &pairs)
}

/// `source` (RGBA8, `width` by `height`) bent by `pins`, the same size.
/// The bend is worked out every `step` pixels and filled in between.
#[must_use]
pub fn warp(source: &[u8], width: u32, height: u32, pins: &[Pin], step: u32) -> Vec<u8> {
    if pins.iter().all(|pin| pin.from == pin.to) || width == 0 || height == 0 {
        return source.to_vec();
    }
    let back: Vec<_> = pins.iter().map(|pin| (pin.to, pin.from)).collect();
    let step = step.max(1);
    let columns = width.div_ceil(step) + 1;
    let rows = height.div_ceil(step) + 1;
    let grid: Vec<[f32; 2]> = (0..rows)
        .flat_map(|row| (0..columns).map(move |column| (column, row)))
        .map(|(column, row)| deform([(column * step) as f32, (row * step) as f32], &back))
        .collect();
    let at = |column: u32, row: u32| grid[(row * columns + column) as usize];
    let mut out = vec![0; source.len()];
    for y in 0..height {
        let (row, fy) = (y / step, (y % step) as f32 / step as f32);
        for x in 0..width {
            let (column, fx) = (x / step, (x % step) as f32 / step as f32);
            let mix = |a: [f32; 2], b: [f32; 2], t: f32| {
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
            };
            let top = mix(at(column, row), at(column + 1, row), fx);
            let bottom = mix(at(column, row + 1), at(column + 1, row + 1), fx);
            let [sx, sy] = mix(top, bottom, fy);
            let pixel = sample(source, width, height, sx, sy);
            let index = ((y * width + x) * 4) as usize;
            out[index..index + 4].copy_from_slice(&pixel);
        }
    }
    out
}

/// The color at `[x, y]` of an RGBA8 picture (pixel corners at whole
/// numbers), bilinear over premultiplied alpha so edges stay clean.
pub(crate) fn sample(source: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let mut sum = [0.0_f32; 3];
    let mut alpha = 0.0_f32;
    for (dx, dy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let (px, py) = (x0 as i64 + dx, y0 as i64 + dy);
        if weight <= 0.0 || px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
            continue;
        }
        let index = ((py as u32 * width + px as u32) * 4) as usize;
        let a = f32::from(source[index + 3]) / 255.0 * weight;
        for (channel, value) in sum.iter_mut().zip(&source[index..index + 3]) {
            *channel += f32::from(*value) * a;
        }
        alpha += a;
    }
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

/// The pixels of an RGBA8 picture that are not clear, as `[x0, y0, x1,
/// y1]` (exclusive), or `None` if it is all clear.
#[must_use]
pub fn bounds(source: &[u8], width: u32, height: u32) -> Option<[u32; 4]> {
    let mut found: Option<[u32; 4]> = None;
    for y in 0..height {
        for x in 0..width {
            if source[((y * width + x) * 4 + 3) as usize] == 0 {
                continue;
            }
            found = Some(match found {
                None => [x, y, x + 1, y + 1],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)],
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn pins_carry_the_picture_rigidly() {
        // No pins and unmoved pins change nothing; one pin translates.
        assert_eq!(deform([3.0, 4.0], &[]), [3.0, 4.0]);
        assert!(close(
            forward([3.0, 4.0], &[Pin::at([0.0, 0.0])]),
            [3.0, 4.0]
        ));
        let moved = Pin {
            from: [0.0, 0.0],
            to: [5.0, -2.0],
        };
        assert!(close(forward([3.0, 4.0], &[moved]), [8.0, 2.0]));
        // Two pins turned a quarter around the first: everything turns.
        let turn = [
            Pin::at([0.0, 0.0]),
            Pin {
                from: [10.0, 0.0],
                to: [0.0, 10.0],
            },
        ];
        assert!(close(forward([5.0, 5.0], &turn), [-5.0, 5.0]));
        assert!(
            close(forward([10.0, 0.0], &turn), [0.0, 10.0]),
            "a pin lands where it was dragged"
        );
        // A point between still and dragged pins moves part way, smoothly.
        let bend = [
            Pin::at([0.0, 0.0]),
            Pin::at([100.0, 0.0]),
            Pin {
                from: [50.0, 0.0],
                to: [50.0, 30.0],
            },
        ];
        let near = forward([45.0, 0.0], &bend);
        let far = forward([5.0, 0.0], &bend);
        assert!(
            near[1] > far[1] && near[1] > 10.0 && far[1] < 5.0,
            "{near:?} {far:?}"
        );
    }

    #[test]
    fn a_warped_layer_moves_its_pixels_and_keeps_pinned_ones() {
        let (width, height) = (64, 64);
        let mut source = vec![0; (width * height * 4) as usize];
        // A red square at 8..16, a blue one at 44..52.
        for y in 8..16 {
            for x in 8..16 {
                source[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        for y in 44..52 {
            for x in 44..52 {
                source[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let still = [Pin::at([12.0, 12.0]), Pin::at([48.0, 48.0])];
        assert_eq!(warp(&source, width, height, &still, 8), source);
        // Hold the red square, drag the blue one up by 20.
        let pins = [
            Pin::at([12.0, 12.0]),
            Pin {
                from: [48.0, 48.0],
                to: [48.0, 28.0],
            },
        ];
        let bent = warp(&source, width, height, &pins, 4);
        let pixel = |buffer: &[u8], x: u32, y: u32| {
            let index = ((y * width + x) * 4) as usize;
            [
                buffer[index],
                buffer[index + 1],
                buffer[index + 2],
                buffer[index + 3],
            ]
        };
        assert_eq!(
            pixel(&bent, 12, 12),
            [255, 0, 0, 255],
            "the held square stays"
        );
        assert_eq!(
            pixel(&bent, 48, 28)[2],
            255,
            "the dragged square came along"
        );
        assert!(pixel(&bent, 48, 48)[3] < 128, "and left its old place");
        assert_eq!(bounds(&source, width, height), Some([8, 8, 52, 52]));
    }
}
