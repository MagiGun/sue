//! Text drawn into pixels: lines laid out across, or in columns down and
//! right to left as manga balloons read, in an installed font, each glyph's
//! outline filled with anti-aliased coverage. Studios that put text in
//! pictures (a paint studio's text layers, titles over video) draw it here.
//!
//! The filling is an accumulation rasterizer after font-rs (Raph Levien,
//! Apache 2.0 or MIT): every outline edge adds its signed area to the cells
//! it crosses, and a running sum along each row gives each pixel's coverage.

use ttf_parser::{Face, OutlineBuilder};

/// Text to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSpec<'a> {
    /// The words, lines apart by `\n`.
    pub text: &'a str,
    /// The font family (empty: Sue's own).
    pub font: &'a str,
    /// Size in pixels (the em).
    pub size: f32,
    /// The ink, sRGB.
    pub rgb: [u8; 3],
    /// Where it starts: the top left corner (top right for columns), or
    /// its middle when `center` is set.
    pub at: [f32; 2],
    /// Centered on `at`, every line centered too.
    pub center: bool,
    /// In columns, top to bottom and right to left.
    pub vertical: bool,
}

/// Edges of a glyph in pixels, as lines.
struct Outline {
    lines: Vec<[[f32; 2]; 2]>,
    start: [f32; 2],
    last: [f32; 2],
    origin: [f32; 2],
    scale: f32,
}

impl Outline {
    fn point(&self, x: f32, y: f32) -> [f32; 2] {
        [
            self.origin[0] + x * self.scale,
            self.origin[1] - y * self.scale,
        ]
    }

    fn line(&mut self, to: [f32; 2]) {
        self.lines.push([self.last, to]);
        self.last = to;
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.start = p;
        self.last = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.line(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (from, control, to) = (self.last, self.point(x1, y1), self.point(x, y));
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            let u = 1.0 - t;
            self.line([
                u * u * from[0] + 2.0 * u * t * control[0] + t * t * to[0],
                u * u * from[1] + 2.0 * u * t * control[1] + t * t * to[1],
            ]);
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (from, c1, c2, to) = (
            self.last,
            self.point(x1, y1),
            self.point(x2, y2),
            self.point(x, y),
        );
        for step in 1..=12 {
            let t = step as f32 / 12.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.line([
                a * from[0] + b * c1[0] + c * c2[0] + d * to[0],
                a * from[1] + b * c1[1] + c * c2[1] + d * to[1],
            ]);
        }
    }

    fn close(&mut self) {
        if self.last != self.start {
            let start = self.start;
            self.line(start);
        }
    }
}

/// Coverage (0 to 1) of `lines` over a `width` by `height` grid whose top
/// left is at `min`: the accumulation rasterizer.
fn fill(lines: &[[[f32; 2]; 2]], min: [f32; 2], width: usize, height: usize) -> Vec<f32> {
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

/// Lays `coverage` of a `w` by `h` box at `min` over the pixels in `rgb`.
fn blend(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    coverage: &[f32],
    min: [i64; 2],
    w: usize,
    rgb: [u8; 3],
) {
    for (index, cover) in coverage.iter().enumerate() {
        if *cover <= 0.0 {
            continue;
        }
        let (x, y) = (min[0] + (index % w) as i64, min[1] + (index / w) as i64);
        if x < 0 || y < 0 || x >= i64::from(width) || y >= i64::from(height) {
            continue;
        }
        let at = ((y as u32 * width + x as u32) * 4) as usize;
        let under = f32::from(pixels[at + 3]) / 255.0;
        let alpha = cover + under * (1.0 - cover);
        for channel in 0..3 {
            let mixed = f32::from(rgb[channel]) * cover
                + f32::from(pixels[at + channel]) * under * (1.0 - cover);
            pixels[at + channel] = (mixed / alpha.max(1e-6)).round().clamp(0.0, 255.0) as u8;
        }
        pixels[at + 3] = (alpha * 255.0).round() as u8;
    }
}

/// Draws one glyph with its origin (on the baseline) at `origin`; returns
/// its box.
#[allow(clippy::too_many_arguments, reason = "a glyph, where, and the pixels it lands on")]
fn glyph(
    face: &Face<'_>,
    id: ttf_parser::GlyphId,
    origin: [f32; 2],
    scale: f32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rgb: [u8; 3],
) -> Option<[f32; 4]> {
    let mut outline = Outline {
        lines: Vec::new(),
        start: origin,
        last: origin,
        origin,
        scale,
    };
    face.outline_glyph(id, &mut outline)?;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for point in outline.lines.iter().flatten() {
        (x0, y0, x1, y1) = (
            x0.min(point[0]),
            y0.min(point[1]),
            x1.max(point[0]),
            y1.max(point[1]),
        );
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let min = [x0.floor() - 1.0, y0.floor() - 1.0];
    let (w, h) = (
        (x1.ceil() - min[0] + 2.0) as usize,
        (y1.ceil() - min[1] + 2.0) as usize,
    );
    let coverage = fill(&outline.lines, min, w, h);
    blend(
        pixels,
        width,
        height,
        &coverage,
        [min[0] as i64, min[1] as i64],
        w,
        rgb,
    );
    Some([x0, y0, x1, y1])
}

/// Draws `spec` over `pixels` (RGBA8, `width` by `height`). Returns the box
/// the text covers, `[x0, y0, x1, y1]`, or `None` when nothing drew (no
/// font, no glyphs).
pub fn draw_text(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    spec: &TextSpec<'_>,
) -> Option<[f32; 4]> {
    let (bytes, index) = sue_wm::fonts::face_bytes(spec.font, false, false)
        .or_else(|| sue_wm::fonts::face_bytes("", false, false))?;
    let face = Face::parse(&bytes, index).ok()?;
    let scale = spec.size / f32::from(face.units_per_em().max(1));
    let ascender = f32::from(face.ascender()) * scale;
    let descender = f32::from(face.descender()) * scale;
    let line_height = (ascender - descender + f32::from(face.line_gap()) * scale).max(spec.size);
    let advance = |ch: char| {
        face.glyph_index(ch)
            .and_then(|id| face.glyph_hor_advance(id))
            .map_or(spec.size * 0.5, |units| f32::from(units) * scale)
    };
    let lines: Vec<&str> = spec.text.split('\n').collect();
    let mut covered: Option<[f32; 4]> = None;
    let mut put = |ch: char, origin: [f32; 2], pixels: &mut [u8]| {
        if let Some(id) = face.glyph_index(ch)
            && let Some([a, b, c, d]) =
                glyph(&face, id, origin, scale, pixels, width, height, spec.rgb)
        {
            covered = Some(covered.map_or([a, b, c, d], |[x0, y0, x1, y1]| {
                [x0.min(a), y0.min(b), x1.max(c), y1.max(d)]
            }));
        }
    };
    if spec.vertical {
        // Columns right to left, each character in an em square.
        let column = spec.size * 1.2;
        let step = spec.size * 1.05;
        let longest = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let (right, top) = if spec.center {
            (
                spec.at[0] + column * lines.len() as f32 / 2.0,
                spec.at[1] - step * longest / 2.0,
            )
        } else {
            (spec.at[0], spec.at[1])
        };
        for (index, line) in lines.iter().enumerate() {
            let middle = right - (index as f32 + 0.5) * column;
            for (row, ch) in line.chars().enumerate() {
                let baseline = top + row as f32 * step + (step + ascender + descender) / 2.0;
                put(ch, [middle - advance(ch) / 2.0, baseline], pixels);
            }
        }
    } else {
        let total = line_height * lines.len() as f32;
        let top = if spec.center {
            spec.at[1] - total / 2.0
        } else {
            spec.at[1]
        };
        for (index, line) in lines.iter().enumerate() {
            let wide: f32 = line.chars().map(advance).sum();
            let mut x = if spec.center {
                spec.at[0] - wide / 2.0
            } else {
                spec.at[0]
            };
            let baseline = top + index as f32 * line_height + ascender;
            for ch in line.chars() {
                put(ch, [x, baseline], pixels);
                x += advance(ch);
            }
        }
    }
    covered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str, vertical: bool) -> TextSpec<'_> {
        TextSpec {
            text,
            font: "",
            size: 32.0,
            rgb: [0, 0, 0],
            at: [10.0, 10.0],
            center: false,
            vertical,
        }
    }

    #[test]
    fn text_fills_its_glyphs_smoothly_across_and_down() {
        let (width, height) = (200, 200);
        let mut pixels = vec![0; (width * height * 4) as usize];
        let [x0, y0, x1, y1] =
            draw_text(&mut pixels, width, height, &spec("Hello", false)).expect("drawn");
        assert!(x1 - x0 > y1 - y0, "a line is wider than tall");
        assert!(x0 >= 9.0 && y0 >= 10.0 && y1 < 60.0);
        let alphas: Vec<u8> = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        assert!(
            alphas.iter().filter(|alpha| **alpha == 255).count() > 100,
            "solid strokes"
        );
        assert!(
            alphas.iter().any(|alpha| (1..255).contains(alpha)),
            "smooth edges"
        );
        // The H's left stem is solid in its middle, and its middle is open above the bar.
        // Columns down: the same letters stand taller than wide.
        let mut column = vec![0; (width * height * 4) as usize];
        let [x0, y0, x1, y1] = draw_text(
            &mut column,
            width,
            height,
            &TextSpec {
                at: [190.0, 10.0],
                ..spec("Hey", true)
            },
        )
        .expect("drawn");
        assert!(y1 - y0 > x1 - x0);
        assert!(x1 <= 190.5, "from the right");
        // Centered text sits around its point.
        let mut centered = vec![0; (width * height * 4) as usize];
        let [x0, _, x1, _] = draw_text(
            &mut centered,
            width,
            height,
            &TextSpec {
                at: [100.0, 100.0],
                center: true,
                ..spec("Hi", false)
            },
        )
        .expect("drawn");
        assert!(((x0 + x1) / 2.0 - 100.0).abs() < 6.0);
    }
}
