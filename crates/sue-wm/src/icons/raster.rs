//! Filling outlines with anti-aliased coverage: the one rasterizer behind
//! Sue's icons (the stroker of this module hands it polygons) and behind
//! the text `sue-type` draws into pictures (it hands it glyph outlines).
//!
//! The filling is an accumulation rasterizer after font-rs (Raph Levien,
//! Apache 2.0 or MIT): every outline edge adds its signed area to the cells
//! it crosses, and a running sum along each row gives each pixel's coverage.

/// Coverage (0 to 1) of `lines` over a `width` by `height` grid whose top
/// left is at `min`: the accumulation rasterizer.
#[must_use]
pub fn fill(lines: &[[[f32; 2]; 2]], min: [f32; 2], width: usize, height: usize) -> Vec<f32> {
    let mut acc = vec![0.0_f32; width * height + width + 2];
    let top = (width as f32 - 1.001).max(0.0);
    for [a, b] in lines {
        let (p0, p1) = (
            [a[0] - min[0], a[1] - min[1]],
            [b[0] - min[0], b[1] - min[1]],
        );
        if (p0[1] - p1[1]).abs() < 1e-6 {
            continue;
        }
        let (dir, p0, p1) = if p0[1] < p1[1] {
            (1.0, p0, p1)
        } else {
            (-1.0, p1, p0)
        };
        let dxdy = (p1[0] - p0[0]) / (p1[1] - p0[1]);
        let mut x = p0[0];
        let first = p0[1].max(0.0);
        if p0[1] < 0.0 {
            x -= p0[1] * dxdy;
        }
        let rows = (p1[1].ceil() as usize).min(height);
        for y in (first as usize)..rows {
            let row = y * width;
            let dy = ((y + 1) as f32).min(p1[1]) - (y as f32).max(p0[1]);
            let next = x + dxdy * dy;
            let d = dy * dir;
            let (x0, x1) = if x < next { (x, next) } else { (next, x) };
            let (x0, x1) = (x0.clamp(0.0, top), x1.clamp(0.0, top));
            let (x0_floor, x1_ceil) = (x0.floor(), x1.ceil());
            let (x0i, x1i) = (x0_floor as usize, x1_ceil as usize);
            if x1i <= x0i + 1 {
                let middle = 0.5 * (x0 + x1) - x0_floor;
                acc[row + x0i] += d - d * middle;
                acc[row + x0i + 1] += d * middle;
            } else {
                let s = (x1 - x0).recip();
                let x0f = x0 - x0_floor;
                let a0 = 0.5 * s * (1.0 - x0f).powi(2);
                let x1f = x1 - x1_ceil + 1.0;
                let am = 0.5 * s * x1f * x1f;
                acc[row + x0i] += d * a0;
                if x1i == x0i + 2 {
                    acc[row + x0i + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0f);
                    acc[row + x0i + 1] += d * (a1 - a0);
                    for xi in x0i + 2..x1i - 1 {
                        acc[row + xi] += d * s;
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    acc[row + x1i - 1] += d * (1.0 - a2 - am);
                }
                acc[row + x1i] += d * am;
            }
            x = next;
        }
    }
    let mut sum = 0.0;
    let mut out = vec![0.0; width * height];
    for (index, cover) in out.iter_mut().enumerate() {
        if index % width == 0 {
            sum = 0.0;
        }
        sum += acc[index];
        *cover = sum.abs().min(1.0);
    }
    out
}
