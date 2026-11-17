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
/// The border's edge is smoothed (`Bool`, on when missing): off, every
/// pixel of the border is either in it or not (Anti-aliasing on border
/// effect).
pub const BORDER_AA: &str = "effect.border.antialias";
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
/// What sets how large the dots are (`Int`): 0 the brightness of the
/// layer, 1 its opacity (see [`TONE_SOURCES`]).
pub const TONE_SOURCE: &str = "effect.tone.source";
/// How many steps of density there are (`Int`): 0 or 1 as many as the
/// pixels have, 2 or more that many (Posterization).
pub const TONE_LEVELS: &str = "effect.tone.levels";
/// The shape of the dots (`Int`, see [`TONE_SHAPES`]).
pub const TONE_SHAPE: &str = "effect.tone.shape";
/// The layer's opacity sets how dense the tone is, instead of fading
/// the dots (`Bool`, Reflect layer opacity).
pub const TONE_REFLECT_OPACITY: &str = "effect.tone.reflect_opacity";
/// Where the dots' grid starts across, in pixels (`Float`, Dot position).
pub const TONE_DOT_X: &str = "effect.tone.dot_x";
/// Where the dots' grid starts down, in pixels (`Float`, Dot position).
pub const TONE_DOT_Y: &str = "effect.tone.dot_y";

/// What can set the size of a tone's dots, in order.
pub const TONE_SOURCES: [&str; 2] = ["Brightness", "Opacity"];

/// The shapes a tone's dots take, in order: Noise ([`TONE_NOISE`])
/// scatters grains instead of drawing dots. Star, then Ellipse, Heart
/// and Flower came later, so they are last and files that keep an index
/// read as they did.
pub const TONE_SHAPES: [&str; 10] = [
    "Circle", "Square", "Diamond", "Line", "Cross", "Noise", "Star", "Ellipse", "Heart", "Flower",
];

/// The size of a noise tone's grains, in pixels (`Float`, 1 when
/// missing: single pixels).
pub const TONE_NOISE_SIZE: &str = "effect.tone.noise_size";
/// How strongly a noise tone's grains stand out, 0 to 1 (`Float`, 1 when
/// missing): at 0 the tone is an even veil of its density, at 1 every
/// grain is either inked or not.
pub const TONE_NOISE_FACTOR: &str = "effect.tone.noise_factor";

/// The index of Noise in [`TONE_SHAPES`].
pub const TONE_NOISE: u8 = 5;

/// The index of Star in [`TONE_SHAPES`].
const TONE_STAR: u8 = 6;
/// The index of Ellipse in [`TONE_SHAPES`].
const TONE_ELLIPSE: u8 = 7;
/// The index of Heart in [`TONE_SHAPES`].
const TONE_HEART: u8 = 8;
/// The index of Flower in [`TONE_SHAPES`].
const TONE_FLOWER: u8 = 9;
/// The layer shown in one color (`Bool`).
pub const COLOR: &str = "effect.color";
/// That color (`Color`).
pub const COLOR_VALUE: &str = "effect.color.value";
/// The sub color (`Color`): when set, the layer's light parts take it and
/// its dark parts the layer color, in between a blend of the two.
pub const COLOR_SUB: &str = "effect.color.sub";
/// Expression color: 0 color, 1 gray, 2 monochrome (`Int`).
pub const EXPRESSION: &str = "effect.expression";
/// Where monochrome turns black, 0 to 1 (`Float`).
pub const THRESHOLD: &str = "effect.threshold";
/// Monochrome may draw black (`Bool`, on when missing); off, what would
/// be black is see-through.
pub const MONO_BLACK: &str = "effect.mono.black";
/// Monochrome may draw white (`Bool`, on when missing); off, what would
/// be white is see-through.
pub const MONO_WHITE: &str = "effect.mono.white";

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
    /// What sets the dots' size: 0 brightness, 1 opacity.
    pub tone_source: u8,
    /// Steps of density (0 or 1: no posterization).
    pub tone_levels: u8,
    /// The dots' shape (an index of [`TONE_SHAPES`]).
    pub tone_shape: u8,
    /// The layer color.
    pub color: Option<[f32; 3]>,
    /// The sub color the layer's light parts take.
    pub sub_color: Option<[f32; 3]>,
    /// 0 color, 1 gray, 2 monochrome.
    pub expression: u8,
    /// Where monochrome turns black.
    pub threshold: f32,
    /// The border's edge is smoothed.
    pub border_aa: bool,
    /// With Reflect layer opacity on, the layer's opacity, which then
    /// sets the tone's density instead of fading the layer.
    pub tone_opacity: Option<f32>,
    /// Where the tone's grid starts, in pixels.
    pub tone_dot: [f32; 2],
    /// Monochrome draws black, and white.
    pub mono: [bool; 2],
    /// A noise tone's grain size in pixels, and how strongly the grains
    /// stand out (0 to 1).
    pub noise: [f32; 2],
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            border: None,
            watercolor: None,
            extract: None,
            tone: None,
            tone_source: 0,
            tone_levels: 0,
            tone_shape: 0,
            color: None,
            sub_color: None,
            expression: 0,
            threshold: 0.5,
            border_aa: true,
            tone_opacity: None,
            tone_dot: [0.0; 2],
            mono: [true; 2],
            noise: [1.0, 1.0],
        }
    }
}

/// A switch stored at `key`, `or` when missing.
fn switch(entity: &Entity, key: &str, or: bool) -> bool {
    entity.property(key).and_then(Value::as_bool).unwrap_or(or)
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

/// An index below `count` stored at `key` (0 when missing or out of
/// range).
fn small(entity: &Entity, key: &str, count: usize) -> u8 {
    entity
        .property(key)
        .and_then(Value::as_i64)
        .filter(|value| (0..count as i64).contains(value))
        .map_or(0, |value| value as u8)
}

/// How much of a tone cell's pixel a dot of `density` (0 to 1) covers:
/// `(du, dv)` is the pixel's place in its cell from the cell's middle, in
/// cells, `cell` the cell's side in pixels.
fn dot(shape: u8, density: f32, du: f32, dv: f32, cell: f32) -> f32 {
    if density <= 0.0 {
        return 0.0;
    }
    if let Some((area, outside)) = polar_shape(shape) {
        // As large as the density says; the shapes of the cells around
        // are drawn too, so a large one is never cut at its cell's edge.
        let scale = (density / area).sqrt();
        let mut cover: f32 = 0.0;
        for nu in [-1.0, 0.0, 1.0] {
            for nv in [-1.0, 0.0, 1.0] {
                let away = outside(du + nu, dv + nv, scale) * cell;
                cover = cover.max((0.5 - away).clamp(0.0, 1.0));
            }
        }
        return cover;
    }
    if shape == TONE_STAR {
        // A star's points reach past its own cell once it is large, so
        // the stars of the cells around are drawn too: a point is never
        // cut off at the cell's edge.
        let outer = (density / STAR_AREA).sqrt();
        let mut cover: f32 = 0.0;
        for nu in [-1.0, 0.0, 1.0] {
            for nv in [-1.0, 0.0, 1.0] {
                let outside = star_outside(du + nu, dv + nv, outer) * cell;
                cover = cover.max((0.5 - outside).clamp(0.0, 1.0));
            }
        }
        return cover;
    }
    let (du, dv) = (du.abs(), dv.abs());
    let (reach, away) = match shape {
        // Square: as large as the density, its side square-rooted.
        1 => (cell * density.sqrt() / 2.0, du.max(dv) * cell),
        // Diamond: a square turned on its point.
        2 => (cell * (density / 2.0).sqrt(), (du + dv) * cell),
        // Line: stripes along the angle.
        3 => (cell * density / 2.0, dv * cell),
        // Cross: two stripes that cover the density together.
        4 => (
            cell * (1.0 - (1.0 - density).max(0.0).sqrt()) / 2.0,
            du.min(dv) * cell,
        ),
        // Circle.
        _ => (
            cell * (density / std::f32::consts::PI).sqrt(),
            du.hypot(dv) * cell,
        ),
    };
    (reach - away + 0.5).clamp(0.0, 1.0)
}

/// How far `(du, dv)` (in cells, `dv` growing downward) lies outside a
/// shape of size `scale`; below zero inside.
type Outside = fn(f32, f32, f32) -> f32;

/// The tone shapes drawn from a curve around their middle: each one's
/// area at size 1 (in cells) and how far a point lies outside it.
fn polar_shape(shape: u8) -> Option<(f32, Outside)> {
    use std::f32::consts::PI;
    match shape {
        // An ellipse lying across, its height 0.6 of its width.
        TONE_ELLIPSE => Some((PI * ELLIPSE_RATIO, |du, dv, scale| {
            let (a, b) = (scale, scale * ELLIPSE_RATIO);
            let reach = (du / a).hypot(dv / b);
            (reach - 1.0) * b.min(a)
        })),
        // A heart, its notch up and its point down, about its middle:
        // r = (2 - 2 sin t + sin t sqrt|cos t| / (sin t + 1.4)) / 2.
        TONE_HEART => Some((HEART_AREA, |du, dv, scale| {
            let up = -dv + HEART_MIDDLE * scale;
            let reach = du.hypot(up);
            let turn = up.atan2(du);
            let (sin, cos) = turn.sin_cos();
            let edge = 0.5 * (2.0 - 2.0 * sin + sin * cos.abs().sqrt() / (sin + 1.4));
            (reach - scale * edge) * 0.5
        })),
        // A flower of five round petals.
        TONE_FLOWER => Some((PI * FLOWER_AREA, |du, dv, scale| {
            let reach = du.hypot(dv);
            let turn = (-dv).atan2(du) - PI / 2.0;
            (reach - scale * (0.72 + 0.28 * (5.0 * turn).cos())) * 0.7
        })),
        _ => None,
    }
}

/// A heart's area at size 1, in cells (measured once, numerically).
const HEART_AREA: f32 = 3.130_76;

/// How far below the heart's curve's own middle its middle of area lies,
/// at size 1.
const HEART_MIDDLE: f32 = -0.535_18;

/// An ellipse dot's height over its width.
const ELLIPSE_RATIO: f32 = 0.6;

/// A flower's area over pi at size 1: the mean of its radius squared.
const FLOWER_AREA: f32 = 0.72 * 0.72 + 0.28 * 0.28 / 2.0;

/// A number from 0 to 1 for picture point `(x, y)` that looks random and
/// is the same every time, in grains `size` pixels across, smoothly
/// between them (the noise tone).
fn grain(x: u32, y: u32, size: f32) -> f32 {
    if size <= 1.0 {
        return scatter(x, y);
    }
    let (fx, fy) = ((x as f32 + 0.5) / size, (y as f32 + 0.5) / size);
    let (gx, gy) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - gx, fy - gy);
    let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let at = |i: f32, j: f32| scatter((gx + i) as i64 as u32, (gy + j) as i64 as u32);
    let top = at(0.0, 0.0) + (at(1.0, 0.0) - at(0.0, 0.0)) * tx;
    let bottom = at(0.0, 1.0) + (at(1.0, 1.0) - at(0.0, 1.0)) * tx;
    // Smoothed values gather round the middle: spread them back out so
    // a density still covers its share.
    let n = top + (bottom - top) * ty;
    (0.5 + (n - 0.5) * 1.7).clamp(0.0, 1.0)
}

/// How far an inner corner of the star lies from its middle, as a share
/// of how far its points reach.
const STAR_INNER: f32 = 0.42;

/// The area of a star whose points reach 1: ten triangles between the
/// middle, a point and an inner corner, 36 degrees apart.
const STAR_AREA: f32 = 5.0 * STAR_INNER * 0.587_785_25;

/// How far `(du, dv)` lies outside a five pointed star whose points
/// reach `outer`, with one point up (`dv` grows downward); below zero
/// inside. All in cells.
fn star_outside(du: f32, dv: f32, outer: f32) -> f32 {
    use std::f32::consts::PI;
    // The angle from the upper point, folded into the half of a point
    // between a point (0) and the next inner corner (36 degrees).
    let half = PI / 5.0;
    let mut turn = du.atan2(-dv).rem_euclid(2.0 * half);
    if turn > half {
        turn = 2.0 * half - turn;
    }
    let reach = du.hypot(dv);
    let (x, y) = (reach * turn.sin(), reach * turn.cos());
    // The edge from the point (0, outer) to the inner corner; its normal
    // faces away from the middle.
    let inner = outer * STAR_INNER;
    let (ex, ey) = (inner * half.sin(), inner * half.cos() - outer);
    let length = ex.hypot(ey).max(f32::EPSILON);
    (x * -ey + (y - outer) * ex) / length
}

/// A number from 0 to 1 for pixel `(x, y)` that looks random and is the
/// same every time (the noise tone).
fn scatter(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1 << 24) as f32
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
            tone_source: small(entity, TONE_SOURCE, TONE_SOURCES.len()),
            tone_levels: entity
                .property(TONE_LEVELS)
                .and_then(Value::as_i64)
                .map_or(0, |value| value.clamp(0, 255) as u8),
            tone_shape: small(entity, TONE_SHAPE, TONE_SHAPES.len()),
            color: on(entity, COLOR).then(|| {
                rgb(
                    entity,
                    COLOR_VALUE,
                    [40.0 / 255.0, 120.0 / 255.0, 220.0 / 255.0],
                )
            }),
            sub_color: on(entity, COLOR)
                .then(|| match entity.property(COLOR_SUB) {
                    Some(Value::Color(color)) => {
                        Some(color.to_space(sue_core::ColorSpace::Srgb).rgb)
                    }
                    _ => None,
                })
                .flatten(),
            expression: entity
                .property(EXPRESSION)
                .and_then(Value::as_i64)
                .map_or(0, |value| value.clamp(0, 2) as u8),
            threshold: number(entity, THRESHOLD, 0.5) as f32,
            border_aa: switch(entity, BORDER_AA, true),
            tone_opacity: (on(entity, TONE) && on(entity, TONE_REFLECT_OPACITY)).then(|| {
                sue_model::picture::LayerStyle::of(entity)
                    .opacity
                    .clamp(0.0, 1.0)
            }),
            tone_dot: [
                number(entity, TONE_DOT_X, 0.0) as f32,
                number(entity, TONE_DOT_Y, 0.0) as f32,
            ],
            mono: [
                switch(entity, MONO_BLACK, true),
                switch(entity, MONO_WHITE, true),
            ],
            noise: [
                number(entity, TONE_NOISE_SIZE, 1.0).clamp(1.0, 64.0) as f32,
                number(entity, TONE_NOISE_FACTOR, 1.0).clamp(0.0, 1.0) as f32,
            ],
        }
    }

    /// How much of a pixel the border covers, from what its edge says:
    /// as it is, or all or nothing with anti-aliasing off.
    #[must_use]
    pub fn border_cover(&self, cover: f32) -> f32 {
        if self.border_aa {
            cover
        } else if cover >= 0.5 {
            1.0
        } else {
            0.0
        }
    }

    /// How opaque the layer is laid on, for its `opacity`: fully when the
    /// tone takes the opacity in as its density (Reflect layer opacity).
    #[must_use]
    pub fn laid_opacity(&self, opacity: f32) -> f32 {
        if self.tone_opacity.is_some() {
            1.0
        } else {
            opacity
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
                let white = luma >= self.threshold;
                let ink = if white { 1.0 } else { 0.0 };
                // A color the layer may not draw is see-through.
                let kept = if white { self.mono[1] } else { self.mono[0] };
                [ink, ink, ink, if kept { a } else { 0.0 }]
            }
            _ => [r, g, b, a],
        };
        if let Some((cell, angle)) = self.tone {
            // Dots on a turned grid, as large as the gray is dark (or the
            // layer opaque), in steps when posterized.
            let mut density = if self.tone_source == 1 {
                a
            } else {
                1.0 - (0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2])
            }
            .clamp(0.0, 1.0);
            if let Some(opacity) = self.tone_opacity {
                density *= opacity;
            }
            if self.tone_levels >= 2 {
                let steps = f32::from(self.tone_levels - 1);
                density = (density * steps).round() / steps;
            }
            let cover = if self.tone_shape == TONE_NOISE {
                // Grains of the noise size, standing out as much as the
                // noise factor says.
                let inked = if grain(x, y, self.noise[0]) < density {
                    1.0
                } else {
                    0.0
                };
                let factor = self.noise[1].clamp(0.0, 1.0);
                density * (1.0 - factor) + inked * factor
            } else {
                let (sin, cos) = angle.sin_cos();
                let (px, py) = (
                    x as f32 + 0.5 - self.tone_dot[0],
                    y as f32 + 0.5 - self.tone_dot[1],
                );
                let (u, v) = ((px * cos + py * sin) / cell, (-px * sin + py * cos) / cell);
                let (hu, hv) = (u + 0.5, v + 0.5);
                match self.tone_shape {
                    _ if density >= 1.0 => 1.0,
                    // Dark dots past half turn into light holes between
                    // them, so the coverage keeps up to solid.
                    0..=2 | TONE_STAR..=TONE_FLOWER if density > 0.5 => {
                        1.0 - dot(
                            self.tone_shape,
                            1.0 - density,
                            hu - hu.round(),
                            hv - hv.round(),
                            cell,
                        )
                    }
                    shape => dot(shape, density, u - u.round(), v - v.round(), cell),
                }
            };
            let alpha = if self.tone_source == 1 { 1.0 } else { a };
            out = [0.0, 0.0, 0.0, alpha * cover];
        }
        if let Some(color) = self.color {
            out = match self.sub_color {
                // The dark parts take the layer color, the light ones the
                // sub color.
                Some(sub) => {
                    let light =
                        (0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2]).clamp(0.0, 1.0);
                    [
                        color[0] + (sub[0] - color[0]) * light,
                        color[1] + (sub[1] - color[1]) * light,
                        color[2] + (sub[2] - color[2]) * light,
                        out[3],
                    ]
                }
                None => [color[0], color[1], color[2], out[3]],
            };
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
    fn ellipse_heart_and_flower_tones_cover_their_density_in_their_own_shapes() {
        // Full guide 00:10:49: the other dot shapes. Each covers about as
        // much as its gray asks, and each looks unlike every other.
        let look = |shape: u8, gray: f32| -> Vec<f32> {
            let effects = Effects {
                tone: Some((16.0, 0.0)),
                tone_shape: shape,
                ..Effects::default()
            };
            (0..64)
                .flat_map(|y| (0..64).map(move |x| (x, y)))
                .map(|(x, y)| effects.apply([gray, gray, gray, 1.0], x, y)[3])
                .collect()
        };
        for shape in [TONE_ELLIPSE, TONE_HEART, TONE_FLOWER] {
            for gray in [0.8_f32, 0.6, 0.3] {
                let share = look(shape, gray).iter().sum::<f32>() / 4096.0;
                assert!(
                    (share - (1.0 - gray)).abs() < 0.08,
                    "{shape} at {gray}: {share}"
                );
            }
            for other in 0..TONE_SHAPES.len() as u8 {
                if other != shape && other != TONE_NOISE {
                    assert_ne!(look(shape, 0.7), look(other, 0.7), "{shape} and {other}");
                }
            }
        }
        // A heart's notch is up: its top row is thinner than its bottom.
        let heart = Effects {
            tone: Some((40.0, 0.0)),
            tone_shape: TONE_HEART,
            ..Effects::default()
        };
        let row = |heart: &Effects, y: u32| -> f32 {
            (0..40)
                .map(|x| heart.apply([0.75, 0.75, 0.75, 1.0], x, y)[3])
                .sum()
        };
        // Its point reaches farther down than its lobes reach up.
        let heart = Effects {
            tone_dot: [20.0, 20.0],
            ..heart
        };
        let inked: Vec<u32> = (0..40).filter(|y| row(&heart, *y) > 0.5).collect();
        let (top, bottom) = (inked[0], inked[inked.len() - 1]);
        assert!(bottom - 20 > 20 - top + 2, "{top} {bottom}");
        // And its notch: the middle of its top row is clear.
        assert!(heart.apply([0.75, 0.75, 0.75, 1.0], 20, top + 1)[3] < 0.5);
    }

    #[test]
    fn noise_size_makes_grains_and_the_factor_softens_them() {
        // Full guide 00:13:54: Noise with its size and factor.
        let look = |size: f32, factor: f32| -> Vec<f32> {
            let effects = Effects {
                tone: Some((8.0, 0.0)),
                tone_shape: TONE_NOISE,
                noise: [size, factor],
                ..Effects::default()
            };
            (0..64)
                .flat_map(|y| (0..64).map(move |x| (x, y)))
                .map(|(x, y)| effects.apply([0.5, 0.5, 0.5, 1.0], x, y)[3])
                .collect()
        };
        // Neighbours agree more often in big grains than in single pixels.
        let same = |cover: &[f32]| {
            (0..64 * 64 - 1)
                .filter(|i| i % 64 != 63 && cover[*i] == cover[i + 1])
                .count()
        };
        let fine = look(1.0, 1.0);
        let coarse = look(6.0, 1.0);
        assert!(
            same(&coarse) > same(&fine) + 800,
            "{} {}",
            same(&coarse),
            same(&fine)
        );
        for cover in [&fine, &coarse] {
            let share = cover.iter().sum::<f32>() / 4096.0;
            assert!((share - 0.5).abs() < 0.1, "{share}");
        }
        // No factor: an even veil of the density.
        assert!(look(6.0, 0.0).iter().all(|c| (c - 0.5).abs() < 1e-4));
        let half = look(6.0, 0.5);
        assert!(
            half.iter()
                .all(|c| (c - 0.25).abs() < 1e-4 || (c - 0.75).abs() < 1e-4)
        );
    }

    #[test]
    fn a_star_tone_draws_dots_of_its_own_shape() {
        // Full guide 00:13:41: a star among the dot shapes. Its dots are
        // none of the other shapes, a star with a point up is not the
        // same turned upside down, and its points are never cut off at
        // the cell's edge.
        let look = |shape: u8, gray: f32| -> Vec<f32> {
            let effects = Effects {
                tone: Some((16.0, 0.0)),
                tone_shape: shape,
                ..Effects::default()
            };
            (0..64)
                .flat_map(|y| (0..64).map(move |x| (x, y)))
                .map(|(x, y)| effects.apply([gray, gray, gray, 1.0], x, y)[3])
                .collect()
        };
        let star = look(TONE_STAR, 0.7);
        assert!(star.iter().any(|cover| *cover > 0.5), "the star draws");
        for other in 0..TONE_STAR {
            assert_ne!(
                look(other, 0.7),
                star,
                "{}",
                TONE_SHAPES[usize::from(other)]
            );
        }
        let at = |x: usize, y: usize| star[y * 64 + x];
        let flipped = (0..64).all(|y| (0..64).all(|x| (at(x, y) - at(x, 63 - y)).abs() < 1e-3));
        assert!(!flipped, "a point up is not a point down");
        let mirrored = (0..64).all(|y| (0..64).all(|x| (at(x, y) - at(63 - x, y)).abs() < 1e-3));
        assert!(mirrored, "the star stands upright");
        // Its area follows the density, light and dark alike.
        for gray in [0.85, 0.7, 0.3, 0.15] {
            let covered = look(TONE_STAR, gray).iter().sum::<f32>() / (64.0 * 64.0);
            assert!((covered - (1.0 - gray)).abs() < 0.04, "{gray}: {covered}");
        }
        // Near half the points reach past the cell, and the cell's edge
        // does not cut them: on the line between two cells the stars
        // still ink where their points cross it.
        let half = look(TONE_STAR, 0.55);
        let edge: f32 = (0..64).map(|y| half[y * 64 + 8]).sum();
        assert!(edge > 1.0, "{edge}");
    }

    #[test]
    fn tones_take_shapes_steps_and_the_opacity_as_density() {
        let share = |effects: &Effects, rgba: [f32; 4]| {
            let mut sum = 0.0;
            for y in 0..96 {
                for x in 0..96 {
                    sum += effects.apply(rgba, x, y)[3];
                }
            }
            sum / (96.0 * 96.0)
        };
        let gray = [0.5, 0.5, 0.5, 1.0];
        // Every shape covers about as much as the gray is dark.
        for shape in 0..TONE_SHAPES.len() as u8 {
            let effects = Effects {
                tone: Some((8.0, 0.4)),
                tone_shape: shape,
                ..Effects::default()
            };
            let covered = share(&effects, gray);
            assert!((covered - 0.5).abs() < 0.08, "{shape}: {covered}");
            // White takes no dot at all.
            assert_eq!(share(&effects, [1.0, 1.0, 1.0, 1.0]), 0.0, "{shape}");
        }
        // Two steps: light grays vanish and dark ones go solid.
        let steps = Effects {
            tone: Some((8.0, 0.4)),
            tone_levels: 2,
            ..Effects::default()
        };
        assert_eq!(share(&steps, [0.7, 0.7, 0.7, 1.0]), 0.0);
        assert!(share(&steps, [0.3, 0.3, 0.3, 1.0]) > 0.97);
        // By opacity: a half clear white layer is half covered.
        let by_alpha = Effects {
            tone: Some((8.0, 0.4)),
            tone_source: 1,
            ..Effects::default()
        };
        let covered = share(&by_alpha, [1.0, 1.0, 1.0, 0.5]);
        assert!((covered - 0.5).abs() < 0.08, "{covered}");
        // The settings read from a layer, out of range ones as defaults.
        let entity = Entity::empty("Tone", "sue.layer")
            .with_property(TONE, true)
            .with_property(TONE_SHAPE, 3_i64)
            .with_property(TONE_SOURCE, 7_i64)
            .with_property(TONE_LEVELS, 4_i64);
        let read = Effects::of(&entity, 72.0);
        assert_eq!(
            (read.tone_shape, read.tone_source, read.tone_levels),
            (3, 0, 4)
        );
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

    #[test]
    fn border_aliasing_monochrome_inks_and_tone_options() {
        let layer = |props: &[(&str, Value)]| {
            let mut entity = Entity::empty("Layer", "layer");
            for (key, value) in props {
                entity.properties_mut().set(*key, value.clone());
            }
            Effects::of(&entity, 72.0)
        };
        // Anti-aliasing on border effect: off, the border's edge is hard.
        let soft = layer(&[(BORDER, Value::from(true))]);
        assert!((soft.border_cover(0.3) - 0.3).abs() < 1e-6);
        let hard = layer(&[(BORDER, Value::from(true)), (BORDER_AA, Value::from(false))]);
        assert_eq!([hard.border_cover(0.3), hard.border_cover(0.7)], [0.0, 1.0]);
        // Monochrome with white turned off: white is see-through, black
        // stays.
        let mono = layer(&[
            (EXPRESSION, Value::from(2_i64)),
            (MONO_WHITE, Value::from(false)),
        ]);
        assert_eq!(mono.apply([1.0, 1.0, 1.0, 1.0], 0, 0)[3], 0.0);
        assert_eq!(mono.apply([0.0, 0.0, 0.0, 1.0], 0, 0), [0.0, 0.0, 0.0, 1.0]);
        let no_black = layer(&[
            (EXPRESSION, Value::from(2_i64)),
            (MONO_BLACK, Value::from(false)),
        ]);
        assert_eq!(no_black.apply([0.0, 0.0, 0.0, 1.0], 0, 0)[3], 0.0);
        // Reflect layer opacity: the layer lays on whole and its opacity
        // thins the dots instead.
        let tone = |reflect: bool| {
            layer(&[
                (TONE, Value::from(true)),
                (TONE_REFLECT_OPACITY, Value::from(reflect)),
                (TONE_SOURCE, Value::from(1_i64)),
                (sue_model::picture::OPACITY, Value::from(0.25)),
            ])
        };
        let covered = |effects: &Effects| {
            (0..64)
                .flat_map(|y| (0..64).map(move |x| (x, y)))
                .map(|(x, y)| effects.apply([0.0, 0.0, 0.0, 1.0], x, y)[3])
                .sum::<f32>()
        };
        assert!((tone(false).laid_opacity(0.25) - 0.25).abs() < 1e-6);
        assert!((tone(true).laid_opacity(0.25) - 1.0).abs() < 1e-6);
        assert!(covered(&tone(true)) < covered(&tone(false)) * 0.5);
        // Dot position moves the grid.
        let at = |dot: f64| {
            let effects = layer(&[
                (TONE, Value::from(true)),
                (TONE_SOURCE, Value::from(1_i64)),
                (TONE_DOT_X, Value::from(dot)),
            ]);
            (0..16)
                .map(|x| effects.apply([0.0, 0.0, 0.0, 0.3], x, 0)[3])
                .collect::<Vec<_>>()
        };
        assert_ne!(at(0.0), at(0.5));
    }
}
