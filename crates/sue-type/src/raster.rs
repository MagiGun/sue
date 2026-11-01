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
    /// Extra room between letters, in ems (0 for the font's own).
    pub tracking: f32,
    /// Line spacing, times the font's own (0 or 1 for the font's own).
    pub leading: f32,
    /// How the lines line up with each other.
    pub justify: Justify,
    /// A curve the words follow, their baseline on it (empty: straight).
    /// Lines after the first follow it further down.
    pub path: &'a [[f32; 2]],
}

/// Edges of a glyph in pixels, as lines.
struct Outline {
    lines: Vec<[[f32; 2]; 2]>,
    start: [f32; 2],
    last: [f32; 2],
    origin: [f32; 2],
    scale: f32,
    /// The glyph's turn: the cosine and sine of its angle.
    turn: [f32; 2],
}

impl Outline {
    fn point(&self, x: f32, y: f32) -> [f32; 2] {
        let (lx, ly) = (x * self.scale, -y * self.scale);
        let [c, s] = self.turn;
        [
            self.origin[0] + lx * c - ly * s,
            self.origin[1] + lx * s + ly * c,
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
#[allow(
    clippy::too_many_arguments,
    reason = "a glyph, where, and the pixels it lands on"
)]
fn glyph(
    face: &Face<'_>,
    id: ttf_parser::GlyphId,
    (origin, turn): ([f32; 2], [f32; 2]),
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
        turn,
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

/// How lines line up with each other: their starts, middles or ends
/// (tops, middles or bottoms in columns).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    /// Lines start together (left, or top in columns).
    #[default]
    Start,
    /// Lines are centered on each other.
    Middle,
    /// Lines end together (right, or bottom in columns).
    End,
}

/// One line laid out: each glyph's origin on the baseline and its turn
/// (cosine and sine), and the caret places before each character and
/// after the last (`[top, bottom]`).
struct LaidLine {
    glyphs: Vec<(char, [f32; 2], [f32; 2])>,
    slots: Vec<[[f32; 2]; 2]>,
}

/// Where along `path` arc length `s` is, and which way the path goes
/// there; before its start and past its end it carries on straight.
fn along(path: &[[f32; 2]], lengths: &[f32], s: f32) -> ([f32; 2], [f32; 2]) {
    let last = path.len() - 1;
    let index = lengths
        .iter()
        .position(|length| *length >= s)
        .unwrap_or(last)
        .clamp(1, last);
    let (a, b) = (path[index - 1], path[index]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy).max(1e-6);
    let tangent = [dx / length, dy / length];
    let t = s - lengths[index - 1];
    ([a[0] + tangent[0] * t, a[1] + tangent[1] * t], tangent)
}

/// [`lay_out`] along `spec.path`: each line's baseline on the curve (later
/// lines further down), each glyph turned with it.
fn lay_out_on_path(spec: &TextSpec<'_>, metrics: &Metrics<'_>) -> Vec<LaidLine> {
    let path = spec.path;
    let leading = if spec.leading > 0.0 {
        spec.leading
    } else {
        1.0
    };
    let line_height = metrics.line_height * leading;
    let tracking = spec.tracking * spec.size;
    // Which way is down from the curve at each of its points.
    let normals: Vec<[f32; 2]> = (0..path.len())
        .map(|index| {
            let (a, b) = (
                path[index.saturating_sub(1)],
                path[(index + 1).min(path.len() - 1)],
            );
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = dx.hypot(dy).max(1e-6);
            [-dy / length, dx / length]
        })
        .collect();
    spec.text
        .split('\n')
        .enumerate()
        .map(|(number, line)| {
            // Each line follows its own curve, the first one moved down,
            // so lines inside a bend do not crowd.
            let below = number as f32 * line_height;
            let curve: Vec<[f32; 2]> = path
                .iter()
                .zip(&normals)
                .map(|(p, n)| [p[0] + n[0] * below, p[1] + n[1] * below])
                .collect();
            let mut lengths = vec![0.0_f32];
            for pair in curve.windows(2) {
                let step = (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
                lengths.push(lengths[lengths.len() - 1] + step);
            }
            let total = lengths[lengths.len() - 1];
            let wide: f32 = line
                .chars()
                .map(|ch| metrics.advance(ch, spec.size) + tracking)
                .sum::<f32>()
                - if line.is_empty() { 0.0 } else { tracking };
            let mut s = match spec.justify {
                Justify::Start => 0.0,
                Justify::Middle => (total - wide) / 2.0,
                Justify::End => total - wide,
            };
            let slot = |s: f32| {
                let (base, tangent) = along(&curve, &lengths, s);
                let normal = [-tangent[1], tangent[0]];
                [
                    [
                        base[0] - normal[0] * metrics.ascender,
                        base[1] - normal[1] * metrics.ascender,
                    ],
                    [
                        base[0] - normal[0] * metrics.descender,
                        base[1] - normal[1] * metrics.descender,
                    ],
                ]
            };
            let mut laid = LaidLine {
                glyphs: Vec::new(),
                slots: Vec::new(),
            };
            for ch in line.chars() {
                let advance = metrics.advance(ch, spec.size);
                laid.slots.push(slot(s));
                // Turned about its middle, so it sits on the curve there.
                let (middle, tangent) = along(&curve, &lengths, s + advance / 2.0);
                laid.glyphs.push((
                    ch,
                    [
                        middle[0] - tangent[0] * advance / 2.0,
                        middle[1] - tangent[1] * advance / 2.0,
                    ],
                    tangent,
                ));
                s += advance + tracking;
            }
            let end = if line.is_empty() { s } else { s - tracking };
            laid.slots.push(slot(end));
            laid
        })
        .collect()
}

/// The font and its sizes for `spec`.
struct Metrics<'a> {
    face: Face<'a>,
    scale: f32,
    ascender: f32,
    descender: f32,
    line_height: f32,
}

impl Metrics<'_> {
    fn advance(&self, ch: char, size: f32) -> f32 {
        self.face
            .glyph_index(ch)
            .and_then(|id| self.face.glyph_hor_advance(id))
            .map_or(size * 0.5, |units| f32::from(units) * self.scale)
    }
}

/// Lays `spec` out as [`draw_text`] draws it.
fn lay_out(spec: &TextSpec<'_>, metrics: &Metrics<'_>) -> Vec<LaidLine> {
    if spec.path.len() >= 2 && !spec.vertical {
        return lay_out_on_path(spec, metrics);
    }
    let lines: Vec<&str> = spec.text.split('\n').collect();
    let leading = if spec.leading > 0.0 {
        spec.leading
    } else {
        1.0
    };
    let tracking = spec.tracking * spec.size;
    let mut out = Vec::with_capacity(lines.len());
    if spec.vertical {
        // Columns right to left, each character in an em square.
        let column = spec.size * 1.2 * leading;
        let step = spec.size * 1.05 + tracking;
        let tall = |line: &str| line.chars().count() as f32 * step;
        let longest = lines.iter().map(|line| tall(line)).fold(0.0, f32::max);
        let (right, top) = if spec.center {
            (
                spec.at[0] + column * lines.len() as f32 / 2.0,
                spec.at[1] - longest / 2.0,
            )
        } else {
            (spec.at[0], spec.at[1])
        };
        for (index, line) in lines.iter().enumerate() {
            let middle = right - (index as f32 + 0.5) * column;
            let start = top
                + match spec.justify {
                    Justify::Start => 0.0,
                    Justify::Middle => (longest - tall(line)) / 2.0,
                    Justify::End => longest - tall(line),
                };
            let mut laid = LaidLine {
                glyphs: Vec::new(),
                slots: Vec::new(),
            };
            for (row, ch) in line.chars().enumerate() {
                let y = start + row as f32 * step;
                laid.slots
                    .push([[middle + column * 0.45, y], [middle - column * 0.45, y]]);
                let baseline = y + (spec.size * 1.05 + metrics.ascender + metrics.descender) / 2.0;
                laid.glyphs.push((
                    ch,
                    [middle - metrics.advance(ch, spec.size) / 2.0, baseline],
                    [1.0, 0.0],
                ));
            }
            let y = start + line.chars().count() as f32 * step;
            laid.slots
                .push([[middle + column * 0.45, y], [middle - column * 0.45, y]]);
            out.push(laid);
        }
    } else {
        let line_height = metrics.line_height * leading;
        let wide = |line: &str| -> f32 {
            line.chars()
                .map(|ch| metrics.advance(ch, spec.size) + tracking)
                .sum::<f32>()
                - if line.is_empty() { 0.0 } else { tracking }
        };
        let widest = lines.iter().map(|line| wide(line)).fold(0.0, f32::max);
        let total = line_height * lines.len() as f32;
        let (left, top) = if spec.center {
            (spec.at[0] - widest / 2.0, spec.at[1] - total / 2.0)
        } else {
            (spec.at[0], spec.at[1])
        };
        for (index, line) in lines.iter().enumerate() {
            let mut x = left
                + match spec.justify {
                    Justify::Start => 0.0,
                    Justify::Middle => (widest - wide(line)) / 2.0,
                    Justify::End => widest - wide(line),
                };
            let y = top + index as f32 * line_height;
            let baseline = y + metrics.ascender;
            let mut laid = LaidLine {
                glyphs: Vec::new(),
                slots: Vec::new(),
            };
            for ch in line.chars() {
                laid.slots.push([[x, y], [x, y + line_height]]);
                laid.glyphs.push((ch, [x, baseline], [1.0, 0.0]));
                x += metrics.advance(ch, spec.size) + tracking;
            }
            let end = if line.is_empty() { x } else { x - tracking };
            laid.slots.push([[end, y], [end, y + line_height]]);
            out.push(laid);
        }
    }
    out
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
    let metrics = metrics(Face::parse(&bytes, index).ok()?, spec.size);
    let mut covered: Option<[f32; 4]> = None;
    for line in lay_out(spec, &metrics) {
        for (ch, origin, turn) in line.glyphs {
            if let Some(id) = metrics.face.glyph_index(ch)
                && let Some([a, b, c, d]) = glyph(
                    &metrics.face,
                    id,
                    (origin, turn),
                    metrics.scale,
                    pixels,
                    width,
                    height,
                    spec.rgb,
                )
            {
                covered = Some(covered.map_or([a, b, c, d], |[x0, y0, x1, y1]| {
                    [x0.min(a), y0.min(b), x1.max(c), y1.max(d)]
                }));
            }
        }
    }
    covered
}

/// The sizes of `face` at `size` pixels to the em.
fn metrics(face: Face<'_>, size: f32) -> Metrics<'_> {
    let scale = size / f32::from(face.units_per_em().max(1));
    let ascender = f32::from(face.ascender()) * scale;
    let descender = f32::from(face.descender()) * scale;
    let line_height = (ascender - descender + f32::from(face.line_gap()) * scale).max(size);
    Metrics {
        face,
        scale,
        ascender,
        descender,
        line_height,
    }
}

/// Where a caret stands before the character at `index` (counted in
/// characters, line breaks too; the last one is after the end): a line
/// from `top` to `bottom`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretSlot {
    /// The character it stands before.
    pub index: usize,
    /// The caret's top end (its right end in columns).
    pub top: [f32; 2],
    /// The caret's bottom end (its left end in columns).
    pub bottom: [f32; 2],
}

/// Every place a caret can stand in `spec`, laid out as [`draw_text`]
/// draws it, in order. Empty when no font loads.
#[must_use]
pub fn caret_slots(spec: &TextSpec<'_>) -> Vec<CaretSlot> {
    let Some((bytes, index)) = sue_wm::fonts::face_bytes(spec.font, false, false)
        .or_else(|| sue_wm::fonts::face_bytes("", false, false))
    else {
        return Vec::new();
    };
    let Ok(face) = Face::parse(&bytes, index) else {
        return Vec::new();
    };
    let metrics = metrics(face, spec.size);
    lay_out(spec, &metrics)
        .into_iter()
        .flat_map(|line| line.slots)
        .enumerate()
        .map(|(index, [top, bottom])| CaretSlot { index, top, bottom })
        .collect()
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
            tracking: 0.0,
            leading: 0.0,
            justify: Justify::Start,
            path: &[],
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

    #[test]
    fn carets_stand_between_the_letters_and_on_each_line() {
        let slots = caret_slots(&spec(
            "Hi
yo", false,
        ));
        // Three places on each line, the break counted once.
        assert_eq!(slots.len(), 6);
        assert_eq!(slots[0].top, [10.0, 10.0]);
        assert!(slots[1].top[0] > slots[0].top[0]);
        assert!(
            slots[3].top[1] > slots[0].top[1],
            "the second line is lower"
        );
        assert_eq!(slots[3].top[0], 10.0, "and starts at the left again");
        assert!(slots.iter().enumerate().all(|(n, slot)| slot.index == n));
        let down = caret_slots(&spec("ab", true));
        assert!(down[1].top[1] > down[0].top[1], "columns go down");
    }

    #[test]
    fn spacing_and_justification_move_letters_and_lines() {
        let plain = spec(
            "ab
abcd", false,
        );
        let slots = caret_slots(&plain);
        let wider = caret_slots(&TextSpec {
            tracking: 0.5,
            ..plain
        });
        // Letters further apart: "b" starts 16 pixels later.
        assert!((wider[1].top[0] - slots[1].top[0] - 16.0).abs() < 0.01);
        let taller = caret_slots(&TextSpec {
            leading: 2.0,
            ..plain
        });
        assert!(taller[3].top[1] - plain.at[1] > (slots[3].top[1] - plain.at[1]) * 1.9);
        // Ends together: the short line moves right, the long one stays.
        let right = caret_slots(&TextSpec {
            justify: Justify::End,
            ..plain
        });
        assert!(right[0].top[0] > slots[0].top[0]);
        assert_eq!(right[3].top[0], slots[3].top[0]);
        assert!(
            (right[2].top[0] - right[7].top[0]).abs() < 0.01,
            "the ends line up"
        );
        let mut pixels = vec![0; 200 * 200 * 4];
        assert!(
            draw_text(
                &mut pixels,
                200,
                200,
                &TextSpec {
                    tracking: 0.5,
                    ..plain
                }
            )
            .is_some()
        );
    }

    #[test]
    fn words_follow_a_curve_turned_with_it() {
        // Down the page: the words go down, so their carets lie across.
        let down = [[50.0, 10.0], [50.0, 190.0]];
        let on_path = TextSpec {
            path: &down,
            ..spec("Hey", false)
        };
        let slots = caret_slots(&on_path);
        assert_eq!(slots.len(), 4);
        assert!(slots[1].top[1] > slots[0].top[1], "along the path");
        assert!(
            (slots[1].top[0] - slots[1].bottom[0]).abs() > 20.0,
            "turned across it"
        );
        let mut pixels = vec![0; 200 * 200 * 4];
        let [x0, y0, x1, y1] = draw_text(&mut pixels, 200, 200, &on_path).expect("drawn");
        assert!(y1 - y0 > x1 - x0, "the line runs down the page");
        // An arc: the middle letter sits on the arc's top.
        let arc: Vec<[f32; 2]> = (0..=40)
            .map(|step| {
                let angle = std::f32::consts::PI * (1.0 + step as f32 / 40.0);
                [100.0 + 80.0 * angle.cos(), 150.0 + 80.0 * angle.sin()]
            })
            .collect();
        let centered = TextSpec {
            path: &arc,
            justify: Justify::Middle,
            ..spec("abc", false)
        };
        let middle = caret_slots(&centered)[1];
        assert!(middle.bottom[1] < 90.0, "up on the arc: {middle:?}");
    }
}
