//! What a stroke's coverage becomes before it meets the layer: the
//! behaviours that work on the stroke as a whole rather than dab by dab.
//!
//! A [`Field`] is a rectangle of the stroke as it stands (coverage, and a
//! color per pixel for a colored stroke). [`Finish`] shapes it, in this
//! order: the dual brush's mask, the paper texture laid once on the whole
//! stroke, the anti-overflow region, the watercolor edge (the inside
//! lighter, the rim darker, then softened) and the halftone of tone
//! scraping. The compositor shows a field of the tiles that changed, the
//! merge lays one of the whole stroke; both come from the same code, so
//! what shows while drawing is what is laid. The watercolor edge reads
//! past the rectangle it is asked for ([`Finish::apron`]), so a field cut
//! at a tile border matches the whole stroke's field there exactly.

use super::color_buffer::ColorBuffer;
use super::math::{cos_sin, smoothstep, unit};
use super::overflow::Region;
use super::settings::{DualMode, Tone, ToneShape, WatercolorEdge};
use crate::buffer::{AREA, StrokeBuffer, TILE, TileKey};
use crate::raster::PixelRect;
use crate::raster::kernel::FULL;
use crate::texture::Texture;

/// A rectangle of a stroke: coverage (0 to 1), how much darker the paint
/// gets (0 to 1, the watercolor rim), and for a colored stroke its
/// straight color, pixel by pixel, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The layer pixels the field covers.
    pub rect: PixelRect,
    /// The coverage of each pixel.
    pub alpha: Vec<f32>,
    /// How much darker each pixel's paint gets.
    pub dark: Vec<f32>,
    /// Each pixel's color, for a stroke whose dabs differ in color.
    pub color: Option<Vec<[f32; 3]>>,
}

impl Field {
    /// An empty field over `rect`.
    #[must_use]
    pub fn empty(rect: PixelRect) -> Self {
        let n = area(rect);
        Self {
            rect,
            alpha: vec![0.0; n],
            dark: vec![0.0; n],
            color: None,
        }
    }

    /// The coverage of `buffer` over `rect`.
    #[must_use]
    pub fn from_coverage(buffer: &StrokeBuffer, rect: PixelRect) -> Self {
        let mut field = Self::empty(rect);
        if rect.is_empty() {
            return field;
        }
        let width = (rect.x1 - rect.x0) as usize;
        let mut tile = vec![0_u16; AREA];
        let (k0, k1) = (
            TileKey::of_pixel(rect.x0, rect.y0),
            TileKey::of_pixel(rect.x1 - 1, rect.y1 - 1),
        );
        for ty in k0.y..=k1.y {
            for tx in k0.x..=k1.x {
                let key = TileKey { x: tx, y: ty };
                if !buffer.tile_coverage(key, &mut tile) {
                    continue;
                }
                let [ox, oy] = key.origin();
                for y in rect.y0.max(oy)..rect.y1.min(oy + TILE) {
                    for x in rect.x0.max(ox)..rect.x1.min(ox + TILE) {
                        let c = tile[((y - oy) * TILE + (x - ox)) as usize];
                        field.alpha[(y - rect.y0) as usize * width + (x - rect.x0) as usize] =
                            f32::from(c) / f32::from(FULL);
                    }
                }
            }
        }
        field
    }

    /// The colored coverage of `buffer` over `rect`.
    #[must_use]
    pub fn from_color(buffer: &ColorBuffer, rect: PixelRect) -> Self {
        let mut field = Self::empty(rect);
        let mut colors = vec![[0.0; 3]; field.alpha.len()];
        let width = (rect.x1 - rect.x0).max(0) as usize;
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let (color, a) = buffer.pixel(x, y);
                let i = (y - rect.y0) as usize * width + (x - rect.x0) as usize;
                field.alpha[i] = a;
                colors[i] = color;
            }
        }
        field.color = Some(colors);
        field
    }

    /// The width in pixels.
    #[must_use]
    pub fn width(&self) -> usize {
        (self.rect.x1 - self.rect.x0).max(0) as usize
    }

    /// The coverage of layer pixel `(x, y)`; 0 outside the field.
    #[must_use]
    pub fn at(&self, x: i32, y: i32) -> f32 {
        let r = self.rect;
        if x < r.x0 || x >= r.x1 || y < r.y0 || y >= r.y1 {
            return 0.0;
        }
        self.alpha[(y - r.y0) as usize * self.width() + (x - r.x0) as usize]
    }

    /// The part of the field over `rect`, which lies inside it.
    fn cropped(&self, rect: PixelRect) -> Self {
        let mut out = Self::empty(rect);
        let (w, ow) = (self.width(), out.width());
        let mut colors = self.color.as_ref().map(|_| vec![[0.0; 3]; out.alpha.len()]);
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let from = (y - self.rect.y0) as usize * w + (x - self.rect.x0) as usize;
                let to = (y - rect.y0) as usize * ow + (x - rect.x0) as usize;
                out.alpha[to] = self.alpha[from];
                out.dark[to] = self.dark[from];
                if let (Some(c), Some(src)) = (&mut colors, &self.color) {
                    c[to] = src[from];
                }
            }
        }
        out.color = colors;
        out
    }
}

/// The pixels of `rect`.
fn area(rect: PixelRect) -> usize {
    if rect.is_empty() {
        0
    } else {
        (rect.x1 - rect.x0) as usize * (rect.y1 - rect.y0) as usize
    }
}

/// `rect` widened by `by` pixels each way.
fn grown(rect: PixelRect, by: i32) -> PixelRect {
    PixelRect {
        x0: rect.x0 - by,
        y0: rect.y0 - by,
        x1: rect.x1 + by,
        y1: rect.y1 + by,
    }
}

/// Box blurs `values` (`width` wide) by `radius` pixels each way, along
/// rows and then columns; what lies outside counts as 0.
fn box_blur(values: &mut [f32], width: usize, radius: usize) {
    if radius == 0 || width == 0 {
        return;
    }
    let height = values.len() / width;
    let span = (2 * radius + 1) as f32;
    let mut line = Vec::new();
    // Rows.
    for row in values.chunks_mut(width) {
        line.clear();
        line.extend_from_slice(row);
        let mut sum: f32 = line.iter().take(radius + 1).sum();
        for x in 0..width {
            row[x] = sum / span;
            if x + radius + 1 < width {
                sum += line[x + radius + 1];
            }
            if x >= radius {
                sum -= line[x - radius];
            }
        }
    }
    // Columns.
    let mut column = vec![0.0; height];
    for x in 0..width {
        for y in 0..height {
            column[y] = values[y * width + x];
        }
        let mut sum: f32 = column.iter().take(radius + 1).sum();
        for y in 0..height {
            values[y * width + x] = sum / span;
            if y + radius + 1 < height {
                sum += column[y + radius + 1];
            }
            if y >= radius {
                sum -= column[y - radius];
            }
        }
    }
}

/// The halftone coverage of a stroke of coverage `a` at pixel center
/// `p`, for `tone` whose screen turn is `(cos, sin)`. The share of a cell
/// the dots fill follows `a`, so a soft airbrush becomes dots that grow
/// from nothing into each other; the dots' rims are antialiased over a
/// pixel.
#[must_use]
pub fn halftone(tone: &Tone, turn: (f32, f32), a: f32, p: [f32; 2]) -> f32 {
    let a = unit(a);
    if a <= 0.0 {
        return 0.0;
    }
    if a >= 1.0 {
        return 1.0;
    }
    let cell = if tone.cell.is_finite() {
        tone.cell.max(2.0)
    } else {
        6.0
    };
    let (c, s) = turn;
    let threshold = |p: [f32; 2]| {
        let u = (c * p[0] + s * p[1]) / cell;
        let v = (-s * p[0] + c * p[1]) / cell;
        let (fu, fv) = (u - u.floor() - 0.5, v - v.floor() - 0.5);
        match tone.shape {
            ToneShape::Line => 2.0 * fv.abs(),
            ToneShape::Dot => dot_threshold(fu, fv),
        }
    };
    // The pixel is painted where `a` passes its threshold; the slope of
    // the threshold over one pixel sets the width of the antialiased
    // rim.
    let here = threshold(p);
    let across = threshold([p[0] + 1.0, p[1]]) - here;
    let down = threshold([p[0], p[1] + 1.0]) - here;
    // A step across a cell border is no slope.
    let clip = |d: f32| if d.abs() > 0.5 { 0.0 } else { d };
    let (across, down) = (clip(across), clip(down));
    let slope = (across * across + down * down).sqrt();
    unit((a - here) / slope.max(1.0e-3) + 0.5)
}

/// Steps of the table [`dot_table`] holds.
const DOT_STEPS: usize = 4096;

/// The share of a cell at or below each step of the dot shape's raw
/// threshold: so the threshold, read through it, fills exactly `a` of
/// every cell.
fn dot_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        const GRID: usize = 512;
        let mut counts = vec![0_u32; DOT_STEPS + 1];
        for j in 0..GRID {
            for i in 0..GRID {
                let fu = (i as f32 + 0.5) / GRID as f32 - 0.5;
                let fv = (j as f32 + 0.5) / GRID as f32 - 0.5;
                let g = dot_raw(fu, fv);
                counts[(g * DOT_STEPS as f32) as usize] += 1;
            }
        }
        let total = (GRID * GRID) as f32;
        let mut sum = 0_u32;
        counts
            .iter()
            .map(|&n| {
                sum += n;
                sum as f32 / total
            })
            .collect()
    })
}

/// The dot shape before it is evened out: 0 at a cell's middle, 1 at its
/// corners, rising smoothly between, so the dots grow round from the
/// middle, meet, and leave round holes that close at the corners.
fn dot_raw(fu: f32, fv: f32) -> f32 {
    let middle = fu * fu + fv * fv;
    let (cu, cv) = (0.5 - fu.abs(), 0.5 - fv.abs());
    let corner = cu * cu + cv * cv;
    (middle / (middle + corner).max(1.0e-12)).clamp(0.0, 1.0)
}

/// The dot shape's threshold at `(fu, fv)` (each -0.5 to 0.5 across the
/// cell): the share of the cell that is painted before this point is.
fn dot_threshold(fu: f32, fv: f32) -> f32 {
    let table = dot_table();
    let g = dot_raw(fu, fv) * DOT_STEPS as f32;
    let i = (g as usize).min(DOT_STEPS - 1);
    let t = g - i as f32;
    let below = if i == 0 { 0.0 } else { table[i - 1] };
    // Within a step the share rises from the step below to this one.
    below + (table[i] - below) * t
}

/// The behaviours that shape a whole stroke.
#[derive(Clone, Copy, Debug, Default)]
pub struct Finish<'a> {
    /// The dual brush's coverage and how it meets the stroke's.
    pub dual: Option<(&'a StrokeBuffer, DualMode)>,
    /// A paper laid once on the whole stroke.
    pub texture: Option<&'a Texture<'a>>,
    /// The region anti-overflow keeps the paint in.
    pub overflow: Option<&'a Region>,
    /// The watercolor edge.
    pub watercolor: Option<WatercolorEdge>,
    /// The halftone.
    pub tone: Option<Tone>,
}

impl Finish<'_> {
    /// How far past a rectangle the shaping reads, in pixels.
    #[must_use]
    pub fn apron(&self) -> i32 {
        self.watercolor.map_or(0, |edge| {
            let reach = |v: f32| {
                if v.is_finite() {
                    v.clamp(0.0, 200.0)
                } else {
                    0.0
                }
            };
            // Two passes of each blur.
            (2.0 * (reach(edge.range).ceil() + reach(edge.blur).ceil())) as i32 + 1
        })
    }

    /// The stroke in `buffer` over `rect`, shaped.
    #[must_use]
    pub fn coverage(&self, buffer: &StrokeBuffer, rect: PixelRect) -> Field {
        let apron = self.apron();
        let mut field = Field::from_coverage(buffer, grown(rect, apron));
        self.shape(&mut field);
        if apron == 0 {
            field
        } else {
            field.cropped(rect)
        }
    }

    /// The colored stroke in `buffer` over `rect`, shaped.
    #[must_use]
    pub fn color(&self, buffer: &ColorBuffer, rect: PixelRect) -> Field {
        let apron = self.apron();
        let mut field = Field::from_color(buffer, grown(rect, apron));
        self.shape(&mut field);
        if apron == 0 {
            field
        } else {
            field.cropped(rect)
        }
    }

    /// Shapes `field` in place.
    pub fn shape(&self, field: &mut Field) {
        let rect = field.rect;
        let width = field.width();
        let center = |i: usize| {
            [
                (rect.x0 + (i % width) as i32) as f32 + 0.5,
                (rect.y0 + (i / width) as i32) as f32 + 0.5,
            ]
        };
        if let Some((dual, mode)) = self.dual {
            let mask = Field::from_coverage(dual, rect);
            for (a, &d) in field.alpha.iter_mut().zip(&mask.alpha) {
                *a = dual_mix(mode, *a, d);
            }
        }
        if let Some(texture) = self.texture {
            for (i, a) in field.alpha.iter_mut().enumerate() {
                if *a > 0.0 {
                    *a = texture.apply(*a, center(i));
                }
            }
        }
        if let Some(region) = self.overflow {
            for (i, a) in field.alpha.iter_mut().enumerate() {
                if *a > 0.0 {
                    let x = rect.x0 + (i % width) as i32;
                    let y = rect.y0 + (i / width) as i32;
                    *a *= region.at(x, y);
                }
            }
        }
        if let Some(edge) = self.watercolor {
            watercolor(field, &edge);
        }
        if let Some(tone) = self.tone {
            let turn = cos_sin(tone.angle);
            for (i, a) in field.alpha.iter_mut().enumerate() {
                *a = halftone(&tone, turn, *a, center(i));
            }
        }
    }
}

/// The stroke's coverage `a` through the dual brush's `d`.
#[must_use]
pub fn dual_mix(mode: DualMode, a: f32, d: f32) -> f32 {
    let (a, d) = (unit(a), unit(d));
    match mode {
        DualMode::Multiply => a * d,
        DualMode::Darken => a.min(d),
        DualMode::Subtract => a * (1.0 - d),
        DualMode::Overlay => {
            if a < 0.5 {
                2.0 * a * d
            } else {
                1.0 - 2.0 * (1.0 - a) * (1.0 - d)
            }
        }
        DualMode::Height => a * smoothstep(-0.08, 0.08, d - (1.0 - a)),
    }
}

/// The watercolor edge on `field`: the rim keeps its paint and darkens,
/// the inside lightens to the edge's opacity, and the result is
/// softened by the edge's blur.
fn watercolor(field: &mut Field, edge: &WatercolorEdge) {
    let width = field.width();
    if width == 0 {
        return;
    }
    let range = if edge.range.is_finite() {
        edge.range.clamp(0.0, 200.0)
    } else {
        0.0
    };
    // Two box passes of half the range make a soft bell about as wide as
    // the range.
    let r = ((range * 0.5).round() as usize).max(1);
    let mut inside = field.alpha.clone();
    box_blur(&mut inside, width, r);
    box_blur(&mut inside, width, r);
    let opacity = unit(edge.opacity);
    let darkness = unit(edge.darkness);
    for ((a, dark), b) in field.alpha.iter_mut().zip(&mut field.dark).zip(&inside) {
        if *a <= 0.0 {
            continue;
        }
        // How much of a rim the pixel is: 1 where the stroke is twice as
        // strong as its surroundings' mean, 0 well inside.
        let rim = unit(2.0 * (*a - b) / *a);
        *a *= opacity + (1.0 - opacity) * rim;
        *dark = darkness * rim;
    }
    let blur = if edge.blur.is_finite() {
        edge.blur.clamp(0.0, 50.0).round() as usize
    } else {
        0
    };
    if blur > 0 {
        box_blur(&mut field.alpha, width, blur);
        box_blur(&mut field.dark, width, blur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Combine;
    use crate::raster::{Disc, RoundTip};
    use crate::texture::{Paper, TextureSettings};

    fn blob() -> StrokeBuffer {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        buffer.committed().capsule(
            Disc::new([30.0, 60.0], 20.0, 1.0),
            Disc::new([130.0, 60.0], 20.0, 1.0),
            RoundTip::hard(1.0),
        );
        buffer
    }

    const WHOLE: PixelRect = PixelRect {
        x0: 0,
        y0: 0,
        x1: 192,
        y1: 128,
    };

    #[test]
    fn the_watercolor_edge_darkens_the_rim_and_lightens_the_inside() {
        let finish = Finish {
            watercolor: Some(WatercolorEdge {
                range: 8.0,
                opacity: 0.4,
                darkness: 0.5,
                blur: 0.0,
                after_stroke: false,
            }),
            ..Finish::default()
        };
        let field = finish.coverage(&blob(), WHOLE);
        let at = |x: i32, y: i32| field.at(x, y);
        let dark = |x: i32, y: i32| field.dark[(y * 192 + x) as usize];
        // The middle is at the edge's opacity, the rim near full.
        assert!((at(80, 60) - 0.4).abs() < 0.02, "{}", at(80, 60));
        assert!(at(80, 41) > 0.8, "{}", at(80, 41));
        assert!(dark(80, 41) > 0.3 && dark(80, 60) < 0.02);
    }

    #[test]
    fn a_field_cut_at_tile_borders_matches_the_whole_stroke() {
        let finish = Finish {
            watercolor: Some(WatercolorEdge::default()),
            ..Finish::default()
        };
        let buffer = blob();
        let whole = finish.coverage(&buffer, WHOLE);
        for key in buffer.keys() {
            let [ox, oy] = key.origin();
            let rect = PixelRect {
                x0: ox,
                y0: oy,
                x1: ox + TILE,
                y1: oy + TILE,
            };
            let part = finish.coverage(&buffer, rect);
            for y in rect.y0..rect.y1 {
                for x in rect.x0..rect.x1 {
                    assert!(
                        (part.at(x, y) - whole.at(x, y)).abs() < 1.0e-5,
                        "({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn every_dual_mode_lays_paint_by_the_second_tip() {
        for &mode in DualMode::ALL {
            let strong = dual_mix(mode, 0.8, 1.0);
            let weak = dual_mix(mode, 0.8, 0.1);
            if mode == DualMode::Subtract {
                // Its second tip is what takes the paint away.
                assert!(weak > strong, "{mode:?}: {weak} against {strong}");
            } else {
                assert!(weak < strong, "{mode:?}: {weak} against {strong}");
            }
            assert_eq!(dual_mix(mode, 0.0, 0.7), 0.0, "{mode:?}");
        }
    }

    #[test]
    fn the_dual_brush_lays_the_stroke_through_its_dabs() {
        let mut dual = StrokeBuffer::new(Combine::Max);
        dual.committed().capsule(
            Disc::new([30.0, 50.0], 4.0, 1.0),
            Disc::new([130.0, 50.0], 4.0, 1.0),
            RoundTip::hard(1.0),
        );
        let finish = Finish {
            dual: Some((&dual, DualMode::Multiply)),
            ..Finish::default()
        };
        let field = finish.coverage(&blob(), WHOLE);
        assert_eq!(field.at(80, 50), 1.0);
        assert_eq!(field.at(80, 70), 0.0);
    }

    #[test]
    fn a_halftone_fills_its_cells_as_the_coverage_says() {
        for &shape in ToneShape::ALL {
            let tone = Tone {
                cell: 8.0,
                angle: 30.0,
                shape,
            };
            let turn = cos_sin(tone.angle);
            for &a in &[0.1_f32, 0.3, 0.5, 0.7, 0.9] {
                let mut sum = 0.0;
                let mut binary = 0;
                for y in 0..96 {
                    for x in 0..96 {
                        let c = halftone(&tone, turn, a, [x as f32 + 0.5, y as f32 + 0.5]);
                        sum += c;
                        binary += usize::from(c == 0.0 || c == 1.0);
                    }
                }
                let mean = sum / (96.0 * 96.0);
                assert!((mean - a).abs() < 0.08, "{shape:?} at {a}: {mean}");
                assert!(
                    binary > 96 * 96 / 2,
                    "{shape:?} at {a}: mostly dots, not gray"
                );
            }
        }
    }

    #[test]
    fn a_stroke_texture_breaks_the_whole_stroke_up() {
        let paper = Paper::builtin("grain").expect("grain");
        let texture = Texture::new(&paper, TextureSettings::default());
        let finish = Finish {
            texture: Some(&texture),
            ..Finish::default()
        };
        let field = finish.coverage(&blob(), WHOLE);
        let inside: Vec<f32> = (40..120).map(|x| field.at(x, 60)).collect();
        let lo = inside.iter().copied().fold(1.0, f32::min);
        assert!(lo < 0.8, "{lo}");
        assert_eq!(field.at(5, 5), 0.0);
    }
}
