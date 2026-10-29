//! Manga and comics, as Clip Studio Paint makes them: frame borders (panel
//! layouts with gutters, divided by dragging a line), speech balloons with
//! tails, focus lines, speed lines and screentone.
//!
//! A frame border layer is a raster layer of class [`FRAME`] that keeps its
//! panels in [`PANELS`] and draws them: clear inside, the border inked along
//! each panel's edge, the gutters between panels in the paper's color, so
//! the layers under it show only inside the panels. A balloon layer
//! ([`BALLOON`]) keeps its ellipses and tails in [`SHAPES`] and draws them as
//! one white shape with one outline. Both draw again from their shapes after
//! every edit, so edits stay sharp.

use crate::brush::{Dynamics, Stroke as BrushStroke, Tip};
use sue_core::{EntityId, PixelFormat, RasterTile2D};

/// The class of a frame border layer.
pub const FRAME: &str = "sue.paint.frame";
/// A frame layer's panels (`Text`, see [`encode_panels`]).
pub const PANELS: &str = "frame.panels";
/// A frame layer's border width in pixels (`Float`).
pub const BORDER: &str = "frame.border";
/// A frame layer's gap between panels when divided, in pixels (`Float`).
pub const GUTTER: &str = "frame.gutter";
/// The class of a balloon layer.
pub const BALLOON: &str = "sue.paint.balloon";
/// A balloon layer's shapes (`Text`, see [`encode_shapes`]).
pub const SHAPES: &str = "balloon.shapes";
/// A balloon layer's outline width in pixels (`Float`).
pub const LINE: &str = "balloon.line";

/// A panel: a convex polygon in picture pixels.
pub type Panel = Vec<[f32; 2]>;

// ----- Frame borders ------------------------------------------------------------

/// Ready-made panel layouts: a name and the panels in each row, top first.
pub const LAYOUTS: [(&str, &[usize]); 7] = [
    ("One panel", &[1]),
    ("Two rows", &[1, 1]),
    ("Three rows", &[1, 1, 1]),
    ("Four rows (4-koma)", &[1, 1, 1, 1]),
    ("Two by two", &[2, 2]),
    ("Manga page (2, 1, 2)", &[2, 1, 2]),
    ("Two by three", &[2, 2, 2]),
];

/// The panels of layout `rows` (panels per row) inside a `width` by
/// `height` page, `margin` from its edges, `gap` apart across and `gap_y`
/// apart down.
#[must_use]
pub fn layout(
    rows: &[usize],
    width: f32,
    height: f32,
    margin: f32,
    gap: f32,
    gap_y: f32,
) -> Vec<Panel> {
    let (x0, y0) = (margin, margin);
    let (inner_w, inner_h) = (
        (width - 2.0 * margin).max(1.0),
        (height - 2.0 * margin).max(1.0),
    );
    let count = rows.len().max(1) as f32;
    let row_h = ((inner_h - gap_y * (count - 1.0)) / count).max(1.0);
    let mut panels = Vec::new();
    for (row, &columns) in rows.iter().enumerate() {
        let columns = columns.max(1) as f32;
        let column_w = ((inner_w - gap * (columns - 1.0)) / columns).max(1.0);
        let top = y0 + row as f32 * (row_h + gap_y);
        for column in 0..columns as usize {
            let left = x0 + column as f32 * (column_w + gap);
            panels.push(vec![
                [left, top],
                [left + column_w, top],
                [left + column_w, top + row_h],
                [left, top + row_h],
            ]);
        }
    }
    panels
}

/// A rectangle panel from two corners.
#[must_use]
pub fn rectangle(a: [f32; 2], b: [f32; 2]) -> Panel {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// How far `p` is outside a convex polygon (negative inside: how far from
/// its nearest edge).
#[must_use]
pub fn signed_distance(polygon: &[[f32; 2]], p: [f32; 2]) -> f32 {
    if polygon.len() < 3 {
        return f32::INFINITY;
    }
    let area: f32 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    let turn = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut out = f32::NEG_INFINITY;
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx.hypot(dy);
        if length < 1e-6 {
            continue;
        }
        // The outward normal of an edge of a polygon turning `turn` ways.
        let (nx, ny) = (dy * turn / length, -dx * turn / length);
        out = out.max((p[0] - a[0]) * nx + (p[1] - a[1]) * ny);
    }
    out
}

/// A convex polygon cut to where `dot(normal, p) >= offset`.
fn clip(polygon: &[[f32; 2]], normal: [f32; 2], offset: f32) -> Panel {
    let side = |p: [f32; 2]| p[0] * normal[0] + p[1] * normal[1] - offset;
    let mut out = Vec::new();
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        let (sa, sb) = (side(*a), side(*b));
        if sa >= 0.0 {
            out.push(*a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            let t = sa / (sa - sb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// The panels with the one under the middle of the line from `a` to `b`
/// cut in two along it, `gap` apart (Clip Studio Paint's frame border
/// division). `None` if the line cuts no panel.
#[must_use]
pub fn divide(panels: &[Panel], a: [f32; 2], b: [f32; 2], gap: f32) -> Option<Vec<Panel>> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy);
    if length < 1.0 {
        return None;
    }
    let normal = [-dy / length, dx / length];
    let middle = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let index = panels
        .iter()
        .position(|panel| signed_distance(panel, middle) < 0.0)?;
    let offset = normal[0] * a[0] + normal[1] * a[1];
    let half = gap.max(0.0) / 2.0;
    let one = clip(&panels[index], normal, offset + half);
    let other = clip(&panels[index], [-normal[0], -normal[1]], -offset + half);
    if one.len() < 3 || other.len() < 3 {
        return None;
    }
    let mut out = panels.to_vec();
    out.splice(index..=index, [one, other]);
    Some(out)
}

/// Frame borders drawn into RGBA8 pixels: clear inside the panels, the
/// border (`border` pixels, black) along each panel's inner edge, and
/// `gutter` (the paper's color) everywhere else.
#[must_use]
pub fn render_frames(
    panels: &[Panel],
    width: u32,
    height: u32,
    border: f32,
    gutter: [u8; 3],
) -> Vec<u8> {
    let mut distance = vec![f32::INFINITY; (width * height) as usize];
    for panel in panels {
        let reach = border + 2.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in panel {
            (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
        }
        let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
        for y in clamp(y0 - reach, height)..clamp(y1 + reach + 1.0, height) {
            for x in clamp(x0 - reach, width)..clamp(x1 + reach + 1.0, width) {
                let index = (y * width + x) as usize;
                let d = signed_distance(panel, [x as f32 + 0.5, y as f32 + 0.5]);
                if d < distance[index] {
                    distance[index] = d;
                }
            }
        }
    }
    let half = border.max(0.0) / 2.0;
    let mut out = vec![0; (width * height * 4) as usize];
    for (index, d) in distance.into_iter().enumerate() {
        let ink = if border > 0.0 {
            (half - (d + half).abs() + 0.5).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let outside = (d + 0.5).clamp(0.0, 1.0);
        let alpha = ink + (1.0 - ink) * outside;
        if alpha <= 0.0 {
            continue;
        }
        let paper = outside * (1.0 - ink) / alpha;
        let pixel = &mut out[index * 4..index * 4 + 4];
        for (channel, value) in pixel.iter_mut().zip(gutter) {
            *channel = (f32::from(value) * paper).round() as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }
    out
}

/// Panels as text: each panel's points as `x,y`, panels apart by `;`.
#[must_use]
pub fn encode_panels(panels: &[Panel]) -> String {
    panels
        .iter()
        .map(|panel| {
            panel
                .iter()
                .map(|[x, y]| format!("{},{}", x.round(), y.round()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Panels read back from [`encode_panels`]; what does not read is left out.
#[must_use]
pub fn decode_panels(text: &str) -> Vec<Panel> {
    text.split(';')
        .map(|panel| {
            panel
                .split_whitespace()
                .filter_map(|pair| {
                    let (x, y) = pair.split_once(',')?;
                    Some([x.parse().ok()?, y.parse().ok()?])
                })
                .collect::<Panel>()
        })
        .filter(|panel| panel.len() >= 3)
        .collect()
}

// ----- Balloons -----------------------------------------------------------------

/// A part of a balloon layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// An ellipse balloon in its box `[x0, y0, x1, y1]`.
    Ellipse([f32; 4]),
    /// A tail: a triangle from a balloon to its point.
    Tail([[f32; 2]; 3]),
}

impl Shape {
    /// How far `p` is outside the shape (negative inside).
    fn distance(&self, p: [f32; 2]) -> f32 {
        match *self {
            Self::Ellipse([x0, y0, x1, y1]) => {
                let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                let (rx, ry) = (((x1 - x0) / 2.0).max(0.5), ((y1 - y0) / 2.0).max(0.5));
                let (dx, dy) = (p[0] - cx, p[1] - cy);
                let k = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
                if k < 1e-6 {
                    return -rx.min(ry);
                }
                // The distance to the rim, from the gradient of k.
                let gradient = ((dx / (rx * rx)).powi(2) + (dy / (ry * ry)).powi(2)).sqrt() / k;
                (k - 1.0) / gradient.max(1e-6)
            }
            Self::Tail(points) => signed_distance(&points, p),
        }
    }

    /// Its box, `[x0, y0, x1, y1]`.
    fn bounds(&self) -> [f32; 4] {
        match *self {
            Self::Ellipse(rect) => rect,
            Self::Tail(points) => {
                let xs = points.map(|p| p[0]);
                let ys = points.map(|p| p[1]);
                [
                    xs.iter().copied().fold(f32::MAX, f32::min),
                    ys.iter().copied().fold(f32::MAX, f32::min),
                    xs.iter().copied().fold(f32::MIN, f32::max),
                    ys.iter().copied().fold(f32::MIN, f32::max),
                ]
            }
        }
    }
}

/// A tail from inside a balloon at `from` to its point at `to`, `width`
/// pixels wide where it leaves the balloon.
#[must_use]
pub fn tail(from: [f32; 2], to: [f32; 2], width: f32) -> Shape {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = dx.hypot(dy).max(1e-3);
    let (nx, ny) = (-dy / length * width / 2.0, dx / length * width / 2.0);
    Shape::Tail([
        [from[0] + nx, from[1] + ny],
        [from[0] - nx, from[1] - ny],
        to,
    ])
}

/// Balloons drawn into RGBA8 pixels: every shape filled white as one, and
/// one black outline `line` pixels wide around them all.
#[must_use]
pub fn render_balloons(shapes: &[Shape], width: u32, height: u32, line: f32) -> Vec<u8> {
    let count = (width * height) as usize;
    let (mut fill, mut edge) = (vec![0.0_f32; count], vec![0.0_f32; count]);
    for shape in shapes {
        let [x0, y0, x1, y1] = shape.bounds();
        let reach = line + 2.0;
        let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
        for y in clamp(y0 - reach, height)..clamp(y1 + reach + 1.0, height) {
            for x in clamp(x0 - reach, width)..clamp(x1 + reach + 1.0, width) {
                let index = (y * width + x) as usize;
                let d = shape.distance([x as f32 + 0.5, y as f32 + 0.5]);
                fill[index] = fill[index].max((0.5 - d).clamp(0.0, 1.0));
                edge[index] = edge[index].max((0.5 - (d - line)).clamp(0.0, 1.0));
            }
        }
    }
    let mut out = vec![0; count * 4];
    for index in 0..count {
        let (white, alpha) = (fill[index], edge[index].max(fill[index]));
        if alpha <= 0.0 {
            continue;
        }
        let shade = (255.0 * white / alpha).round() as u8;
        out[index * 4..index * 4 + 4].copy_from_slice(&[
            shade,
            shade,
            shade,
            (alpha * 255.0).round() as u8,
        ]);
    }
    out
}

/// Balloon shapes as text: `e:x0,y0,x1,y1` and `t:x,y,x,y,x,y`, apart by `;`.
#[must_use]
pub fn encode_shapes(shapes: &[Shape]) -> String {
    let numbers = |values: &[f32]| {
        values
            .iter()
            .map(|value| value.round().to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    shapes
        .iter()
        .map(|shape| match shape {
            Shape::Ellipse(rect) => format!("e:{}", numbers(rect)),
            Shape::Tail(points) => format!("t:{}", numbers(&points.concat())),
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Balloon shapes read back from [`encode_shapes`].
#[must_use]
pub fn decode_shapes(text: &str) -> Vec<Shape> {
    text.split(';')
        .filter_map(|part| {
            let (kind, values) = part.trim().split_once(':')?;
            let values: Vec<f32> = values
                .split(',')
                .filter_map(|value| value.parse().ok())
                .collect();
            match (kind, values.as_slice()) {
                ("e", &[x0, y0, x1, y1]) => Some(Shape::Ellipse([x0, y0, x1, y1])),
                ("t", &[ax, ay, bx, by, cx, cy]) => {
                    Some(Shape::Tail([[ax, ay], [bx, by], [cx, cy]]))
                }
                _ => None,
            }
        })
        .collect()
}

// ----- Lines and tone, as filters -------------------------------------------------

/// A steady chance from 0 to 1.
fn chance(seed: u32, index: u32, salt: u32) -> f32 {
    let mut h = seed ^ index.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// Draws a line from `from` (full width) to `to` (a point) with Sue's pen,
/// thinning as pressure falls, `profile` giving the pressure along it.
fn ink_line(
    raster: &mut RasterTile2D,
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
    rgb: [u8; 3],
    profile: impl Fn(f32) -> f32,
) {
    let tip = Tip {
        size: width.max(0.5),
        size_by: Dynamics::pressure(0.0),
        ..Tip::default()
    };
    let color = rgb.map(|value| f32::from(value) / 255.0);
    let mut stroke = BrushStroke::new(EntityId::new(), tip, color, false);
    let length = (to[0] - from[0]).hypot(to[1] - from[1]);
    let steps = (length / 6.0).ceil().max(2.0) as u32;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let at = [
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        ];
        stroke.to(raster, at, profile(t).clamp(0.0, 1.0), None);
    }
}

/// Paints with lines on an RGBA8 buffer through a raster.
fn with_raster(pixels: &mut [u8], width: u32, height: u32, paint: impl FnOnce(&mut RasterTile2D)) {
    let Ok(mut raster) =
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels.to_vec())
    else {
        return;
    };
    paint(&mut raster);
    if let Some(bytes) = raster.bytes() {
        pixels.copy_from_slice(bytes);
    }
}

/// Focus lines (Clip Studio Paint's saturated lines): `count` lines from
/// beyond the edges toward `center`, stopping short of a clear middle
/// `gap` of the way out (0 to 1), `width` pixels at their base, their
/// lengths varying by `jitter` (0 to 1).
#[allow(clippy::too_many_arguments, reason = "a filter's settings")]
pub fn focus_lines(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    center: [f32; 2],
    count: u32,
    gap: f32,
    line: f32,
    jitter: f32,
    seed: u32,
    rgb: [u8; 3],
) {
    let far = (width as f32).hypot(height as f32);
    let near = gap.clamp(0.0, 1.0) * (width.min(height) as f32) / 2.0;
    with_raster(pixels, width, height, |raster| {
        for index in 0..count {
            let angle = (index as f32 + (chance(seed, index, 1) - 0.5) * 0.8) / count.max(1) as f32
                * std::f32::consts::TAU;
            let reach = near * (1.0 + jitter.clamp(0.0, 1.0) * chance(seed, index, 2) * 1.5);
            let (dx, dy) = (angle.cos(), angle.sin());
            let from = [center[0] + dx * far, center[1] + dy * far];
            let to = [center[0] + dx * reach, center[1] + dy * reach];
            let thick = line * (0.5 + chance(seed, index, 3));
            ink_line(raster, from, to, thick, rgb, |t| 1.0 - t);
        }
    });
}

/// Speed lines (Clip Studio Paint's streamlines): `count` parallel lines at
/// `angle` degrees across the picture, tapering at both ends, their
/// lengths varying by `jitter` (0 to 1).
#[allow(clippy::too_many_arguments, reason = "a filter's settings")]
pub fn speed_lines(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    count: u32,
    angle: f32,
    line: f32,
    jitter: f32,
    seed: u32,
    rgb: [u8; 3],
) {
    let (w, h) = (width as f32, height as f32);
    let radians = angle.to_radians();
    let (dx, dy) = (radians.cos(), radians.sin());
    let (nx, ny) = (-dy, dx);
    let span = w.hypot(h);
    let center = [w / 2.0, h / 2.0];
    with_raster(pixels, width, height, |raster| {
        for index in 0..count {
            let across = (chance(seed, index, 1) - 0.5) * span;
            let along = (chance(seed, index, 2) - 0.5) * span * 0.6;
            let length = span * (0.6 - 0.45 * jitter.clamp(0.0, 1.0) * chance(seed, index, 3));
            let middle = [
                center[0] + nx * across + dx * along,
                center[1] + ny * across + dy * along,
            ];
            let from = [middle[0] - dx * length / 2.0, middle[1] - dy * length / 2.0];
            let to = [middle[0] + dx * length / 2.0, middle[1] + dy * length / 2.0];
            let thick = line * (0.4 + chance(seed, index, 4));
            ink_line(raster, from, to, thick, rgb, |t| {
                (t * std::f32::consts::PI).sin()
            });
        }
    });
}

/// Screentone: dots on a grid `cell` pixels apart turned `angle` degrees,
/// covering `density` (0 to 1) of the area, laid over the pixels in `rgb`.
pub fn tone(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    density: f32,
    cell: f32,
    angle: f32,
    rgb: [u8; 3],
) {
    let cell = cell.max(2.0);
    let radius = cell * (density.clamp(0.0, 1.0) / std::f32::consts::PI).sqrt();
    let (sin, cos) = angle.to_radians().sin_cos();
    for y in 0..height {
        for x in 0..width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Into the grid's turned frame, then to the nearest dot.
            let (u, v) = (px * cos + py * sin, -px * sin + py * cos);
            let (du, dv) = (u - (u / cell).round() * cell, v - (v / cell).round() * cell);
            let cover = (radius - du.hypot(dv) + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let at = ((y * width + x) * 4) as usize;
            let pixel = &mut pixels[at..at + 4];
            let under = f32::from(pixel[3]) / 255.0;
            let alpha = cover + under * (1.0 - cover);
            for (channel, value) in pixel.iter_mut().zip(rgb) {
                let mixed = f32::from(value) * cover + f32::from(*channel) * under * (1.0 - cover);
                *channel = (mixed / alpha.max(1e-6)).round().clamp(0.0, 255.0) as u8;
            }
            pixel[3] = (alpha * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(pixels: &[u8], width: u32, x: u32, y: u32) -> u8 {
        pixels[((y * width + x) * 4 + 3) as usize]
    }

    #[test]
    fn frames_lay_out_divide_and_draw_with_gutters() {
        let panels = layout(&[2, 1], 200.0, 300.0, 10.0, 8.0, 12.0);
        assert_eq!(panels.len(), 3);
        assert_eq!(panels[0][0], [10.0, 10.0]);
        // Divide the bottom panel down the middle with a 10 pixel gutter.
        let divided = divide(&panels, [100.0, 150.0], [100.0, 295.0], 10.0).unwrap();
        assert_eq!(divided.len(), 4);
        assert!(signed_distance(&divided[2], [90.0, 250.0]) < 0.0);
        assert!(signed_distance(&divided[3], [110.0, 250.0]) < 0.0);
        assert!(
            signed_distance(&divided[2], [100.0, 250.0]) > 0.0,
            "the gutter is open"
        );
        assert!(divide(&panels, [-50.0, -50.0], [-40.0, -40.0], 10.0).is_none());
        // Clear inside, inked edges, paper in the gutters.
        let pixels = render_frames(&divided, 200, 300, 3.0, [255, 255, 255]);
        assert_eq!(alpha(&pixels, 200, 50, 60), 0, "inside a panel");
        assert_eq!(
            &pixels[((60 * 200 + 11) * 4) as usize..][..4],
            &[0, 0, 0, 255],
            "the border"
        );
        assert_eq!(
            &pixels[((60 * 200 + 100) * 4) as usize..][..4],
            &[255, 255, 255, 255],
            "a gutter"
        );
        assert_eq!(decode_panels(&encode_panels(&divided)).len(), 4);
    }

    #[test]
    fn balloons_merge_into_one_outlined_shape() {
        let shapes = vec![
            Shape::Ellipse([20.0, 20.0, 120.0, 80.0]),
            tail([70.0, 70.0], [100.0, 130.0], 20.0),
        ];
        let pixels = render_balloons(&shapes, 160, 160, 3.0);
        let pixel = |x: u32, y: u32| &pixels[((y * 160 + x) * 4) as usize..][..4];
        assert_eq!(pixel(70, 50), &[255, 255, 255, 255], "white inside");
        assert_eq!(pixel(70, 18)[0], 0, "a black outline just outside");
        assert_eq!(
            pixel(70, 81),
            &[255, 255, 255, 255],
            "no line where the tail joins"
        );
        assert_eq!(pixel(5, 5)[3], 0, "clear outside");
        assert_eq!(decode_shapes(&encode_shapes(&shapes)).len(), 2);
    }

    #[test]
    fn focus_and_speed_lines_and_tone_paint() {
        let (width, height) = (120, 90);
        let mut pixels = vec![0; (width * height * 4) as usize];
        focus_lines(
            &mut pixels,
            width,
            height,
            [60.0, 45.0],
            40,
            0.5,
            4.0,
            0.3,
            7,
            [0, 0, 0],
        );
        assert_eq!(alpha(&pixels, width, 60, 45), 0, "the middle stays clear");
        let inked = (0..width)
            .filter(|&x| alpha(&pixels, width, x, 2) > 128)
            .count();
        assert!(inked > 5, "lines reach the edges: {inked}");
        let mut lines = vec![0; (width * height * 4) as usize];
        speed_lines(&mut lines, width, height, 30, 0.0, 3.0, 0.2, 3, [0, 0, 0]);
        assert!(
            lines
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 128)
                .count()
                > 100
        );
        let mut toned = vec![0; (width * height * 4) as usize];
        tone(&mut toned, width, height, 0.3, 8.0, 45.0, [0, 0, 0]);
        let covered = toned
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| f32::from(pixel[3]) / 255.0)
            .sum::<f32>()
            / (width * height) as f32;
        assert!((covered - 0.3).abs() < 0.06, "about 30% covered: {covered}");
    }
}
