//! The Decoration tool, as in Clip Studio Paint: brushes that lay shapes
//! along the stroke instead of a line. Stars, hearts and snow scatter
//! about it; a chain, a lace or stitches run along it, turning with it.
//!
//! Decorations are ordinary brush tips of Sue's one brush engine: a picture
//! shape (registered with [`crate::brush::register_shape`], so every brush
//! may use it) stamped with the tip's spacing, scatter, size and turn
//! jitter, and turning with the line. The shapes are drawn here from their
//! geometry, so they are Sue's own and crisp at any size.

use std::f32::consts::{PI, TAU};

use crate::brush::{TipShape, register_shape};

/// How many samples a shape has across.
const SIDE: u32 = 64;

/// Whether a point, in a shape's square (-1 to 1 each way, the line
/// running along x), is inside the shape.
type Inside = fn(f32, f32) -> bool;

/// Every decoration shape: its name and its inside.
const SHAPES: [(&str, Inside); 16] = [
    ("Star", star),
    ("Heart", heart),
    ("Sparkle", sparkle),
    ("Snowflake", snowflake),
    ("Flower", flower),
    ("Leaf", leaf),
    ("Bubble", bubble),
    ("Chain link", chain_link),
    ("Scallop", scallop),
    ("Stitch", stitch),
    ("Dot", dot),
    ("Rope", rope),
    ("Rickrack", rickrack),
    ("Film strip", film_strip),
    ("Checkered band", checkered_band),
    ("Stitched ribbon", stitched_ribbon),
];

/// A shape's coverage of each sample: the share of 4 by 4 points inside.
fn coverage(inside: Inside) -> Vec<f32> {
    let side = SIDE as usize;
    let mut cover = vec![0.0; side * side];
    for y in 0..side {
        for x in 0..side {
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let u = ((x as f32 + (sx as f32 + 0.5) / 4.0) / SIDE as f32) * 2.0 - 1.0;
                    let v = ((y as f32 + (sy as f32 + 0.5) / 4.0) / SIDE as f32) * 2.0 - 1.0;
                    if inside(u, v) {
                        hits += 1;
                    }
                }
            }
            cover[y * side + x] = hits as f32 / 16.0;
        }
    }
    cover
}

/// The number of decoration shape `name`, made and registered the first
/// time it is asked for.
#[must_use]
pub fn shape(name: &str) -> u32 {
    static IDS: std::sync::OnceLock<Vec<(&'static str, u32)>> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        SHAPES
            .iter()
            .map(|(name, inside)| {
                let id = register_shape(TipShape {
                    name: (*name).to_owned(),
                    side: SIDE,
                    cover: coverage(*inside),
                });
                (*name, id)
            })
            .collect()
    });
    ids.iter()
        .find(|(known, _)| *known == name)
        .map_or(0, |(_, id)| *id)
}

/// `true` if `(u, v)` is inside the polygon of `points`.
fn in_polygon(u: f32, v: f32, points: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let ((xi, yi), (xj, yj)) = (points[i], points[j]);
        if (yi > v) != (yj > v) && u < (xj - xi) * (v - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The points of a star of `tips` tips, `outer` and `inner` radii, the
/// first tip up.
fn star_points(tips: usize, outer: f32, inner: f32) -> Vec<(f32, f32)> {
    (0..tips * 2)
        .map(|at| {
            let angle = -PI / 2.0 + at as f32 * PI / tips as f32;
            let r = if at % 2 == 0 { outer } else { inner };
            (angle.cos() * r, angle.sin() * r)
        })
        .collect()
}

/// How far `(u, v)` is from the segment from `a` to `b`.
fn to_segment(u: f32, v: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((u - a.0) * dx + (v - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (u - a.0 - dx * t).hypot(v - a.1 - dy * t)
}

fn star(u: f32, v: f32) -> bool {
    in_polygon(u, v, &star_points(5, 0.97, 0.4))
}

fn heart(u: f32, v: f32) -> bool {
    let (x, y) = (u * 1.25, -v * 1.25 + 0.2);
    (x * x + y * y - 1.0).powi(3) - x * x * y.powi(3) <= 0.0
}

fn sparkle(u: f32, v: f32) -> bool {
    in_polygon(u, v, &star_points(4, 0.97, 0.16))
}

fn snowflake(u: f32, v: f32) -> bool {
    (0..6).any(|arm| {
        let angle = arm as f32 * TAU / 6.0;
        let (c, s) = (angle.cos(), angle.sin());
        let tip = (c * 0.95, s * 0.95);
        if to_segment(u, v, (0.0, 0.0), tip) < 0.07 {
            return true;
        }
        // Two branches on each arm, spreading out.
        let fork = (c * 0.55, s * 0.55);
        [-0.8_f32, 0.8].iter().any(|turn| {
            let (bc, bs) = ((angle + turn).cos(), (angle + turn).sin());
            to_segment(u, v, fork, (fork.0 + bc * 0.3, fork.1 + bs * 0.3)) < 0.06
        })
    })
}

fn flower(u: f32, v: f32) -> bool {
    let r = u.hypot(v);
    if r < 0.17 {
        // The middle, open.
        return false;
    }
    r < 0.3
        || (0..5).any(|petal| {
            let angle = -PI / 2.0 + petal as f32 * TAU / 5.0;
            (u - angle.cos() * 0.52).hypot(v - angle.sin() * 0.52) < 0.42
        })
}

fn leaf(u: f32, v: f32) -> bool {
    // Two arcs meeting in points at either end, along the line, with a
    // midrib left open.
    let lens = u.hypot(v - 0.62) < 1.0 && u.hypot(v + 0.62) < 1.0;
    lens && !(v.abs() < 0.035 && u.abs() < 0.62)
}

fn bubble(u: f32, v: f32) -> bool {
    let r = u.hypot(v);
    (0.78..0.95).contains(&r) || (u + 0.38).hypot(v + 0.38) < 0.13
}

fn chain_link(u: f32, v: f32) -> bool {
    let outer = (u / 0.95).powi(2) + (v / 0.52).powi(2) <= 1.0;
    let inner = (u / 0.66).powi(2) + (v / 0.24).powi(2) <= 1.0;
    outer && !inner
}

fn scallop(u: f32, v: f32) -> bool {
    // A half ring hanging under the line, a hole in each.
    let r = u.hypot(v + 0.1);
    (v >= -0.1 && (0.62..0.92).contains(&r)) || (u.hypot(v - 0.38) < 0.12)
}

fn stitch(u: f32, v: f32) -> bool {
    u.abs() < 0.9 && v.abs() < 0.16
}

fn dot(u: f32, v: f32) -> bool {
    u.hypot(v) < 0.92
}

// Ribbons: one repeat of a band, laid end to end along the stroke.

fn rope(u: f32, v: f32) -> bool {
    // Strands twisting across the band, a gap between each.
    v.abs() < 0.8 && ((u - v * 0.55) * 1.5 + 4.0).rem_euclid(1.0) < 0.72
}

fn rickrack(u: f32, v: f32) -> bool {
    // A zigzag braid: a ridge down and up once in each repeat.
    let ridge = (u.abs() * 2.0 - 1.0) * 0.55;
    (v - ridge).abs() < 0.2
}

fn film_strip(u: f32, v: f32) -> bool {
    let band = v.abs() < 0.95;
    let hole = v.abs() > 0.7 && v.abs() < 0.88 && u.abs() < 0.3;
    let frame = v.abs() < 0.55 && u.abs() < 0.86;
    band && !hole && !frame
}

fn checkered_band(u: f32, v: f32) -> bool {
    v.abs() < 0.9 && ((u > 0.0) != (v > 0.0))
}

fn stitched_ribbon(u: f32, v: f32) -> bool {
    let rails = (0.72..0.9).contains(&v.abs());
    let stitch = v.abs() < 0.1 && u.abs() < 0.5;
    rails || stitch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_is_drawn_and_registered_once() {
        for (name, inside) in SHAPES {
            let cover = coverage(inside);
            let filled = cover.iter().filter(|value| **value > 0.5).count();
            assert!(
                filled > 60 && filled < (SIDE * SIDE) as usize * 9 / 10,
                "{name}: {filled}"
            );
            let id = shape(name);
            assert_ne!(id, 0, "{name}");
            assert_eq!(shape(name), id, "{name} keeps its number");
        }
        // A flower is open in the middle, a chain link through its eye.
        assert!(!flower(0.0, 0.0) && flower(0.0, -0.7));
        assert!(!chain_link(0.0, 0.0) && chain_link(0.8, 0.0));
        assert_eq!(shape("nothing"), 0);
    }

    /// Every Decoration sub tool painting a wave on a sheet, in rows.
    fn sampler() -> (Vec<u8>, u32, u32) {
        use crate::brush::{Finish, Stroke};
        let (width, height) = (720_u32, 70 * 18);
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).unwrap();
        for (row, sub) in crate::tools::Tool::Decoration
            .sub_tools()
            .iter()
            .enumerate()
        {
            let mut stroke =
                Stroke::new(sue_core::EntityId::new(), sub.tip, [0.0, 0.0, 0.0], false);
            let y = 35.0 + row as f32 * 70.0;
            for step in 0..=120 {
                let x = 40.0 + step as f32 * 5.3;
                let wave = (step as f32 / 120.0 * TAU).sin() * 14.0;
                stroke.to(&mut raster, [x, y + wave], 1.0, None);
            }
            stroke.end(&mut raster, Finish::default(), None);
        }
        (raster.bytes().unwrap().to_vec(), width, height)
    }

    #[test]
    fn each_decoration_paints_along_its_stroke() {
        let (bytes, width, _) = sampler();
        for row in 0..18_u32 {
            let inked = (row * 70..row * 70 + 70)
                .flat_map(|y| (0..width).map(move |x| (y * width + x) as usize * 4 + 3))
                .filter(|at| bytes[*at] > 0)
                .count();
            assert!(inked > 400, "row {row}: {inked}");
        }
        if let Ok(path) = std::env::var("SUE_DECORATION_SAMPLER") {
            let (bytes, width, height) = sampler();
            let mut white = bytes.clone();
            for pixel in white.chunks_mut(4) {
                let a = u16::from(pixel[3]);
                for channel in &mut pixel[..3] {
                    *channel = (255 - a + u16::from(*channel) * a / 255) as u8;
                }
                pixel[3] = 255;
            }
            let png = sue_io::encode_png_rgba(width, height, &white).unwrap();
            std::fs::write(path, png).unwrap();
        }
    }
}
