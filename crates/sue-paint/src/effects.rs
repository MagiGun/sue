//! Layer effects, as in Clip Studio Paint's Layer Property palette: Border
//! effect, Watercolor edge, Extract line, Tone, Layer color and Expression
//! color. Each is a property of
//! the layer that the compositor applies as it draws, so the layer's
//! pixels stay as they are and the effect can be turned off again.

use sue_core::{Color, Entity, Value};

/// A border drawn round the layer (`Bool`).
pub const BORDER: &str = "effect.border";
/// The border's width in pixels (`Float`).
pub const BORDER_WIDTH: &str = "effect.border.width";
/// The border's color (`Color`).
pub const BORDER_COLOR: &str = "effect.border.color";
/// The paint's edges darkened, as watercolor dries (`Bool`).
pub const WATERCOLOR: &str = "effect.watercolor";
/// How far in from the edge it darkens, in pixels (`Float`).
pub const WATERCOLOR_WIDTH: &str = "effect.watercolor.width";
/// How dark the edge gets, 0 to 1 (`Float`).
pub const WATERCOLOR_DARKNESS: &str = "effect.watercolor.darkness";
/// The layer shown as the lines of what it holds (`Bool`).
pub const EXTRACT: &str = "effect.extract";
/// How faint an edge still becomes a line, 0 to 1 (`Float`).
pub const EXTRACT_THRESHOLD: &str = "effect.extract.threshold";
/// The lines' width in pixels (`Float`).
pub const EXTRACT_WIDTH: &str = "effect.extract.width";
/// The layer shown as halftone dots (`Bool`).
pub const TONE: &str = "effect.tone";
/// The dots' screen frequency in lines an inch (`Float`).
pub const TONE_LINES: &str = "effect.tone.lines";
/// The dots' angle in degrees (`Float`).
pub const TONE_ANGLE: &str = "effect.tone.angle";
/// The layer shown in one color (`Bool`).
pub const COLOR: &str = "effect.color";
/// That color (`Color`).
pub const COLOR_VALUE: &str = "effect.color.value";
/// Expression color: 0 color, 1 gray, 2 monochrome (`Int`).
pub const EXPRESSION: &str = "effect.expression";
/// Where monochrome turns black, 0 to 1 (`Float`).
pub const THRESHOLD: &str = "effect.threshold";

/// The expression colors, in order.
pub const EXPRESSIONS: [&str; 3] = ["Color", "Gray", "Monochrome"];

/// A layer's effects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    /// The border's width and color.
    pub border: Option<(f32, [f32; 3])>,
    /// Watercolor edge: how far in it darkens, and how much.
    pub watercolor: Option<(f32, f32)>,
    /// Extract line: the threshold and the lines' width.
    pub extract: Option<(f32, f32)>,
    /// The tone's cell size in pixels and angle in radians.
    pub tone: Option<(f32, f32)>,
    /// The layer color.
    pub color: Option<[f32; 3]>,
    /// 0 color, 1 gray, 2 monochrome.
    pub expression: u8,
    /// Where monochrome turns black.
    pub threshold: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            border: None,
            watercolor: None,
            extract: None,
            tone: None,
            color: None,
            expression: 0,
            threshold: 0.5,
        }
    }
}

fn rgb(entity: &Entity, key: &str, or: [f32; 3]) -> [f32; 3] {
    match entity.property(key) {
        Some(Value::Color(color)) => color.to_space(sue_core::ColorSpace::Srgb).rgb,
        _ => or,
    }
}

fn on(entity: &Entity, key: &str) -> bool {
    entity
        .property(key)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn number(entity: &Entity, key: &str, or: f64) -> f64 {
    entity.property(key).and_then(Value::as_f64).unwrap_or(or)
}

/// The default border color, layer color and their `Color` values.
#[must_use]
pub fn default_color(key: &str) -> Color {
    if key == BORDER_COLOR {
        Color::srgb8(255, 255, 255)
    } else {
        Color::srgb8(40, 120, 220)
    }
}

impl Effects {
    /// The effects of the layer `entity` on a picture of `dpi`.
    #[must_use]
    pub fn of(entity: &Entity, dpi: f64) -> Self {
        let lines = number(entity, TONE_LINES, 60.0).max(1.0);
        Self {
            border: on(entity, BORDER).then(|| {
                (
                    number(entity, BORDER_WIDTH, 4.0).max(0.0) as f32,
                    rgb(entity, BORDER_COLOR, [1.0; 3]),
                )
            }),
            watercolor: on(entity, WATERCOLOR).then(|| {
                (
                    number(entity, WATERCOLOR_WIDTH, 6.0).clamp(1.0, 200.0) as f32,
                    number(entity, WATERCOLOR_DARKNESS, 0.5).clamp(0.0, 1.0) as f32,
                )
            }),
            extract: on(entity, EXTRACT).then(|| {
                (
                    number(entity, EXTRACT_THRESHOLD, 0.25).clamp(0.01, 1.0) as f32,
                    number(entity, EXTRACT_WIDTH, 1.0).clamp(0.5, 10.0) as f32,
                )
            }),
            tone: on(entity, TONE).then(|| {
                (
                    ((dpi / lines) as f32).max(3.0),
                    (number(entity, TONE_ANGLE, 45.0) as f32).to_radians(),
                )
            }),
            color: on(entity, COLOR).then(|| {
                rgb(
                    entity,
                    COLOR_VALUE,
                    [40.0 / 255.0, 120.0 / 255.0, 220.0 / 255.0],
                )
            }),
            expression: entity
                .property(EXPRESSION)
                .and_then(Value::as_i64)
                .map_or(0, |value| value.clamp(0, 2) as u8),
            threshold: number(entity, THRESHOLD, 0.5) as f32,
        }
    }

    /// `true` when the layer shows as it is.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.border.is_none()
            && self.watercolor.is_none()
            && self.extract.is_none()
            && self.tone.is_none()
            && self.color.is_none()
            && self.expression == 0
    }

    /// `true` when an effect looks at a pixel's neighbors (a border, a
    /// watercolor edge, extracted lines), so the layer is drawn a
    /// rectangle at a time.
    #[must_use]
    pub fn needs_area(&self) -> bool {
        self.border.is_some() || self.watercolor.is_some() || self.extract.is_some()
    }

    /// `rgba` (straight, 0 to 1) at picture pixel `(x, y)` through the
    /// expression color, the tone and the layer color.
    #[must_use]
    pub fn apply(&self, [r, g, b, a]: [f32; 4], x: u32, y: u32) -> [f32; 4] {
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let mut out = match self.expression {
            1 => [luma, luma, luma, a],
            2 => {
                let ink = if luma < self.threshold { 0.0 } else { 1.0 };
                [ink, ink, ink, a]
            }
            _ => [r, g, b, a],
        };
        if let Some((cell, angle)) = self.tone {
            // Dots on a turned grid, as large as the gray is dark.
            let density = 1.0 - (0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2]);
            let (sin, cos) = angle.sin_cos();
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (u, v) = ((px * cos + py * sin) / cell, (-px * sin + py * cos) / cell);
            let away = (u - u.round()).hypot(v - v.round()) * cell;
            let radius = cell * (density.clamp(0.0, 1.0) / std::f32::consts::PI).sqrt();
            let cover = (radius - away + 0.5).clamp(0.0, 1.0);
            out = [0.0, 0.0, 0.0, a * cover];
        }
        if let Some(color) = self.color {
            out = [color[0], color[1], color[2], out[3]];
        }
        out
    }
}

/// The lines of what a layer holds over a rectangle (Extract line): how
/// much ink each pixel takes, where the layer's color or coverage changes
/// sharply.
#[derive(Clone, Debug, PartialEq)]
pub struct Lines {
    rect: [u32; 4],
    ink: Vec<f32>,
}

impl Lines {
    /// The lines of `rgba(x, y)` (straight, 0 to 1) over `rect`: edges
    /// stronger than `threshold` become ink, `width` pixels wide.
    #[must_use]
    pub fn build(
        rgba: impl Fn(i64, i64) -> [f32; 4],
        rect: [u32; 4],
        threshold: f32,
        width: f32,
    ) -> Self {
        let reach = width.ceil() as i64 + 1;
        let (x0, y0) = (i64::from(rect[0]) - reach, i64::from(rect[1]) - reach);
        let (x1, y1) = (i64::from(rect[2]) + reach, i64::from(rect[3]) + reach);
        let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        // Each pixel's lightness over white, so clear reads as paper.
        let mut light = vec![1.0_f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let [r, g, b, a] = rgba(x0 + x as i64, y0 + y as i64);
                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                light[y * w + x] = luma * a + (1.0 - a);
            }
        }
        let at = |x: usize, y: usize| light[y.min(h - 1) * w + x.min(w - 1)];
        // Sobel: how sharply the lightness changes.
        let mut edge = vec![0.0_f32; w * h];
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                    - at(x - 1, y - 1)
                    - 2.0 * at(x - 1, y)
                    - at(x - 1, y + 1);
                let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                    - at(x - 1, y - 1)
                    - 2.0 * at(x, y - 1)
                    - at(x + 1, y - 1);
                let strength = gx.hypot(gy) / 4.0;
                edge[y * w + x] =
                    ((strength - threshold) / threshold.max(0.05) + 0.5).clamp(0.0, 1.0);
            }
        }
        // As wide as asked: the strongest edge within the line's radius.
        let radius = ((width - 1.0) / 2.0).max(0.0);
        let spread = radius.ceil() as i64;
        let (rw, rh) = (
            rect[2].saturating_sub(rect[0]) as usize,
            rect[3].saturating_sub(rect[1]) as usize,
        );
        let mut ink = Vec::with_capacity(rw * rh);
        for y in 0..rh {
            for x in 0..rw {
                let (cx, cy) = (x as i64 + reach, y as i64 + reach);
                let mut most: f32 = 0.0;
                for dy in -spread..=spread {
                    for dx in -spread..=spread {
                        if ((dx * dx + dy * dy) as f32).sqrt() > radius + 0.5 {
                            continue;
                        }
                        let (sx, sy) = (cx + dx, cy + dy);
                        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                            continue;
                        }
                        most = most.max(edge[sy as usize * w + sx as usize]);
                    }
                }
                ink.push(most);
            }
        }
        Self { rect, ink }
    }

    /// The ink at picture pixel `(x, y)`, 0 outside the rectangle.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        let [x0, y0, x1, y1] = self.rect;
        if x < x0 || y < y0 || x >= x1 || y >= y1 {
            return 0.0;
        }
        self.ink[((y - y0) * (x1 - x0) + (x - x0)) as usize]
    }
}

/// How much border covers each pixel of a rectangle: 1 where the layer
/// is, falling off over the border's width round it.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    rect: [u32; 4],
    cover: Vec<f32>,
}

/// The squared distance transform of `f` in one dimension (Felzenszwalb
/// and Huttenlocher), in place.
fn distance_1d(f: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let input: Vec<f32> = f.to_vec();
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        if !input[q].is_finite() {
            continue;
        }
        loop {
            let p = v[k];
            let s = if input[p].is_finite() {
                ((input[q] + (q * q) as f32) - (input[p] + (p * p) as f32))
                    / (2.0 * (q as f32 - p as f32))
            } else {
                f32::NEG_INFINITY
            };
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if !input[p].is_finite() {
                // Nothing to the left yet: start from here.
                v[k] = q;
                z[k] = f32::NEG_INFINITY;
                z[k + 1] = f32::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::INFINITY;
            break;
        }
    }
    let mut k = 0;
    for (q, out) in f.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        *out = if input[p].is_finite() {
            let d = q as f32 - p as f32;
            d * d + input[p]
        } else {
            f32::INFINITY
        };
    }
}

impl Edge {
    /// The border `width` pixels wide round where `alpha(x, y)` is at
    /// least a half, over `rect` (`[x0, y0, x1, y1]`).
    #[must_use]
    pub fn build(alpha: impl Fn(i64, i64) -> f32, rect: [u32; 4], width: f32) -> Self {
        Self::shaped(alpha, rect, width, |d| (width + 0.5 - d).clamp(0.0, 1.0))
    }

    /// The same, fading from full at the shape to nothing `width` pixels
    /// away (the watercolor edge's darkening, from the paint's rim in).
    #[must_use]
    pub fn fading(alpha: impl Fn(i64, i64) -> f32, rect: [u32; 4], width: f32) -> Self {
        Self::shaped(alpha, rect, width, |d| {
            let t = (1.0 - d / width.max(0.5)).clamp(0.0, 1.0);
            t * t
        })
    }

    fn shaped(
        alpha: impl Fn(i64, i64) -> f32,
        rect: [u32; 4],
        width: f32,
        cover_of: impl Fn(f32) -> f32,
    ) -> Self {
        let reach = width.ceil() as i64 + 1;
        let (x0, y0) = (i64::from(rect[0]) - reach, i64::from(rect[1]) - reach);
        let (x1, y1) = (i64::from(rect[2]) + reach, i64::from(rect[3]) + reach);
        let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        let mut grid = vec![f32::INFINITY; w * h];
        for y in 0..h {
            for x in 0..w {
                if alpha(x0 + x as i64, y0 + y as i64) >= 0.5 {
                    grid[y * w + x] = 0.0;
                }
            }
        }
        let longest = w.max(h) + 1;
        let (mut v, mut z) = (vec![0_usize; longest], vec![0.0_f32; longest + 1]);
        let mut column = vec![0.0_f32; h];
        for x in 0..w {
            for y in 0..h {
                column[y] = grid[y * w + x];
            }
            distance_1d(&mut column, &mut v, &mut z);
            for y in 0..h {
                grid[y * w + x] = column[y];
            }
        }
        for row in grid.chunks_mut(w.max(1)) {
            distance_1d(row, &mut v, &mut z);
        }
        let (rw, rh) = (
            rect[2].saturating_sub(rect[0]) as usize,
            rect[3].saturating_sub(rect[1]) as usize,
        );
        let mut cover = Vec::with_capacity(rw * rh);
        for y in 0..rh {
            for x in 0..rw {
                let d = grid[(y + reach as usize) * w + x + reach as usize].sqrt();
                cover.push(cover_of(d));
            }
        }
        Self { rect, cover }
    }

    /// The border's cover at picture pixel `(x, y)`, 0 outside the rectangle.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        let [x0, y0, x1, y1] = self.rect;
        if x < x0 || y < y0 || x >= x1 || y >= y1 {
            return 0.0;
        }
        self.cover[((y - y0) * (x1 - x0) + (x - x0)) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watercolor_edges_darken_inside_and_lines_trace_shapes() {
        // A filled square from 10 to 30.
        let inside = |x: i64, y: i64| (10..30).contains(&x) && (10..30).contains(&y);
        // The watercolor edge fades in from the outside.
        let rim = Edge::fading(
            |x, y| if inside(x, y) { 0.0 } else { 1.0 },
            [0, 0, 40, 40],
            4.0,
        );
        assert!(rim.at(10, 20) > 0.5, "the edge itself");
        assert!(
            rim.at(12, 20) > 0.0 && rim.at(12, 20) < rim.at(10, 20),
            "fading in"
        );
        assert_eq!(rim.at(20, 20), 0.0, "the middle keeps its color");
        // Extracted lines follow the square's outline, not its middle.
        let lines = Lines::build(
            |x, y| {
                if inside(x, y) {
                    [0.8, 0.2, 0.2, 1.0]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                }
            },
            [0, 0, 40, 40],
            0.1,
            1.0,
        );
        assert!(
            lines.at(10, 20) > 0.5 || lines.at(9, 20) > 0.5,
            "the outline"
        );
        assert_eq!(lines.at(20, 20), 0.0, "no line in the middle");
        assert_eq!(lines.at(2, 2), 0.0, "none outside");
        let effects = Effects {
            watercolor: Some((4.0, 0.5)),
            ..Effects::default()
        };
        assert!(!effects.is_plain() && effects.needs_area());
    }

    #[test]
    fn borders_grow_round_the_layer_and_effects_recolor_it() {
        // A 4 by 4 square at (10, 10): a 3 pixel border round it.
        let alpha = |x: i64, y: i64| {
            if (10..14).contains(&x) && (10..14).contains(&y) {
                1.0
            } else {
                0.0
            }
        };
        let edge = Edge::build(alpha, [0, 0, 30, 30], 3.0);
        assert_eq!(edge.at(12, 12), 1.0, "under the layer");
        assert_eq!(edge.at(8, 12), 1.0, "two pixels out");
        assert!(edge.at(7, 12) < 1.0 && edge.at(7, 12) > 0.0, "the soft rim");
        assert_eq!(edge.at(6, 12), 0.0);
        assert_eq!(edge.at(3, 12), 0.0, "past the border");
        assert_eq!(edge.at(40, 40), 0.0);
        // Diagonal: round, not square.
        assert!(edge.at(7, 7) < edge.at(8, 12));
        // A part of a picture sees the layer outside it too.
        let part = Edge::build(alpha, [15, 10, 20, 14], 3.0);
        assert_eq!(part.at(15, 12), 1.0);
        assert_eq!(part.at(19, 12), 0.0);

        let mut effects = Effects::default();
        assert!(effects.is_plain());
        effects.expression = 1;
        let gray = effects.apply([1.0, 0.0, 0.0, 1.0], 0, 0);
        assert!((gray[0] - 0.2126).abs() < 1e-4 && gray[0] == gray[2]);
        effects.expression = 2;
        assert_eq!(
            effects.apply([0.3, 0.3, 0.3, 0.5], 0, 0),
            [0.0, 0.0, 0.0, 0.5]
        );
        assert_eq!(
            effects.apply([0.8, 0.8, 0.8, 1.0], 0, 0),
            [1.0, 1.0, 1.0, 1.0]
        );
        effects.expression = 0;
        effects.color = Some([0.0, 0.0, 1.0]);
        assert_eq!(
            effects.apply([1.0, 1.0, 0.0, 0.7], 0, 0),
            [0.0, 0.0, 1.0, 0.7]
        );
        // Tone: a 50% gray becomes dots covering about half.
        let tone = Effects {
            tone: Some((8.0, std::f32::consts::FRAC_PI_4)),
            ..Effects::default()
        };
        let mut sum = 0.0;
        for y in 0..64 {
            for x in 0..64 {
                sum += tone.apply([0.5, 0.5, 0.5, 1.0], x, y)[3];
            }
        }
        let share = sum / 4096.0;
        assert!((share - 0.5).abs() < 0.06, "{share}");
    }

    #[test]
    fn the_compositor_draws_a_border_and_keeps_the_pixels() {
        use crate::doc;
        let mut arena = sue_core::Arena::new();
        let (mut edit, picture) = doc::create(&doc::NewPicture {
            width: 40,
            height: 20,
            ..doc::NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let layer = doc::layers(&arena, picture).last().unwrap().id;
        let raster = doc::raster_mut(&mut arena, layer).unwrap();
        let bytes = raster.bytes_mut().unwrap();
        for y in 8..12 {
            for x in 18..22 {
                bytes[((y * 40 + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let before = doc::raster(&arena, layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec();
        for (key, value) in [
            (BORDER, Value::Bool(true)),
            (BORDER_WIDTH, Value::Float(3.0)),
            (BORDER_COLOR, Value::from(sue_core::Color::srgb8(255, 0, 0))),
        ] {
            let mut set = doc::set(layer, key, value);
            sue_core::ReversibleTransaction::apply(&mut set, &mut arena).unwrap();
        }
        let shown =
            crate::composite::flatten(&arena, picture, crate::composite::RenderOptions::default())
                .unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * 40 + x) * 4) as usize;
            [
                shown.bytes().unwrap()[i],
                shown.bytes().unwrap()[i + 1],
                shown.bytes().unwrap()[i + 2],
            ]
        };
        assert_eq!(at(20, 10), [0, 0, 0], "the dot");
        assert_eq!(at(16, 10), [255, 0, 0], "its border");
        assert_eq!(at(5, 10), [255, 255, 255], "the paper");
        assert_eq!(
            doc::raster(&arena, layer).unwrap().bytes().unwrap(),
            &before[..],
            "pixels as they were"
        );
    }
}
