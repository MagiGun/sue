//! Drawing a [`Drawing`] read from a pack into a tile, the way Sue's own
//! icons are drawn ([`super::super::raster::render`]): cut into straight
//! pieces a fiftieth of a pixel true, fitted to the pixel grid, stroked
//! with round caps and joins and filled with coverage by area.
//!
//! A pack's pen widths are its own: a stroke that is 1.5 units wide in
//! the file is drawn as wide as Sue's own pen at every size (including
//! the heavier pen of small sizes), and a wider or thinner one in
//! proportion. So a pack made from Sue's own set draws Sue's own pixels.

use super::super::WEIGHT;
use super::super::fit::{CANVAS, Fit};
use super::super::path::{Point, Polyline};
use super::super::raster::Tile;
use super::super::stroke::Coverage;
use super::svg::{Cut, Drawing, Item, Paint, Shape};

/// A shape in straight pieces.
struct Flat {
    paint: Paint,
    lines: Vec<Polyline>,
}

enum FlatItem {
    Shape(Flat),
    Masked {
        items: Vec<FlatItem>,
        mask: Vec<(Flat, bool)>,
    },
}

/// Twice the signed area of `line`: positive when it runs clockwise on
/// the screen.
fn area(line: &Polyline) -> f32 {
    let count = line.points.len();
    (0..count)
        .map(|index| {
            let (a, b) = (line.points[index], line.points[(index + 1) % count]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Whether `point` is inside `line`, by the even and odd rule.
fn inside(point: Point, line: &Polyline) -> bool {
    let count = line.points.len();
    let mut within = false;
    for index in 0..count {
        let (a, b) = (line.points[index], line.points[(index + 1) % count]);
        if (a[1] > point[1]) != (b[1] > point[1]) {
            let x = a[0] + (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if point[0] < x {
                within = !within;
            }
        }
    }
    within
}

/// Turns the outlines of an even and odd fill so that the filler, which
/// counts windings, sees the same holes: an outline inside an odd number
/// of others runs the other way round from one inside an even number.
fn even_odd(lines: &mut [Polyline]) {
    let depths: Vec<usize> = (0..lines.len())
        .map(|index| {
            let Some(probe) = lines[index].points.first().copied() else {
                return 0;
            };
            (0..lines.len())
                .filter(|other| *other != index && lines[*other].points.len() > 2)
                .filter(|other| inside(probe, &lines[*other]))
                .count()
        })
        .collect();
    for (line, depth) in lines.iter_mut().zip(depths) {
        let clockwise = area(line) > 0.0;
        if clockwise == (depth % 2 == 1) {
            line.points.reverse();
        }
    }
}

fn flatten(shape: &Shape, tolerance: f32) -> Flat {
    let mut lines: Vec<Polyline> = shape
        .paths
        .iter()
        .map(|path| path.flatten(tolerance))
        .collect();
    if let Paint::Fill { even_odd: true } = shape.paint {
        even_odd(&mut lines);
    }
    Flat {
        paint: shape.paint,
        lines,
    }
}

fn flatten_items(items: &[Item], tolerance: f32) -> Vec<FlatItem> {
    items
        .iter()
        .map(|item| match item {
            Item::Shape(shape) => FlatItem::Shape(flatten(shape, tolerance)),
            Item::Masked { items, mask } => FlatItem::Masked {
                items: flatten_items(items, tolerance),
                mask: mask
                    .iter()
                    .map(|Cut { shape, keep }| (flatten(shape, tolerance), *keep))
                    .collect(),
            },
        })
        .collect()
}

/// Every shape that is drawn (not those of masks), in order.
fn drawn<'a>(items: &'a [FlatItem], out: &mut Vec<&'a Flat>) {
    for item in items {
        match item {
            FlatItem::Shape(flat) => out.push(flat),
            FlatItem::Masked { items, .. } => drawn(items, out),
        }
    }
}

/// Moves every point of `items` by `fit`.
fn place(items: &mut [FlatItem], fit: &Fit) {
    let moved = |flat: &mut Flat| {
        for line in &mut flat.lines {
            for point in &mut line.points {
                *point = fit.point(*point);
            }
        }
    };
    for item in items {
        match item {
            FlatItem::Shape(flat) => moved(flat),
            FlatItem::Masked { items, mask } => {
                place(items, fit);
                for (flat, _) in mask {
                    moved(flat);
                }
            }
        }
    }
}

/// Lays `flat` onto `tile` with Sue's pen `stroke` pixels wide.
fn lay(tile: &mut Coverage, flat: &Flat, stroke: f32) {
    match flat.paint {
        Paint::Stroke(width) => {
            let half = stroke * (width / WEIGHT) / 2.0;
            for line in &flat.lines {
                tile.stroke(line, half);
            }
        }
        Paint::Fill { .. } => tile.fill(&flat.lines),
    }
}

fn paint_items(items: &[FlatItem], size: usize, stroke: f32) -> Coverage {
    let mut tile = Coverage::new(size);
    for item in items {
        match item {
            FlatItem::Shape(flat) => lay(&mut tile, flat, stroke),
            FlatItem::Masked { items, mask } => {
                let mut seen = paint_items(items, size, stroke);
                let mut keep = Coverage::new(size);
                for (flat, light) in mask {
                    if *light {
                        lay(&mut keep, flat, stroke);
                    } else {
                        let mut cut = Coverage::new(size);
                        lay(&mut cut, flat, stroke);
                        keep.clear(&cut);
                    }
                }
                for ((cell, kept), under) in
                    seen.cells.iter_mut().zip(&keep.cells).zip(&mut tile.cells)
                {
                    *cell *= kept;
                    *under = under.max(*cell);
                }
            }
        }
    }
    tile
}

/// The width that most of the drawn strokes have, in units: the one the
/// pixel fitting is made for.
fn common_width(shapes: &[&Flat]) -> Option<f32> {
    let mut counts: Vec<(f32, usize)> = Vec::new();
    for flat in shapes {
        if let Paint::Stroke(width) = flat.paint {
            match counts.iter_mut().find(|(seen, _)| *seen == width) {
                Some((_, count)) => *count += 1,
                None => counts.push((width, 1)),
            }
        }
    }
    let most = counts.iter().map(|(_, count)| *count).max()?;
    counts
        .iter()
        .find(|(_, count)| *count == most)
        .map(|(width, _)| *width)
}

/// Draws `drawing` on a tile `size` pixels wide, where Sue's own pen is
/// `stroke` pixels wide.
#[must_use]
pub fn render(drawing: &Drawing, size: usize, stroke: f32) -> Tile {
    let size = size.max(1);
    let stroke = if stroke.is_finite() {
        stroke.max(0.0)
    } else {
        0.0
    };
    let tolerance = 0.02 * CANVAS / size as f32;
    let mut items = flatten_items(&drawing.items, tolerance);
    let fit = {
        let mut shapes = Vec::new();
        drawn(&items, &mut shapes);
        let of = |stroked: bool| -> Vec<Polyline> {
            shapes
                .iter()
                .filter(|flat| matches!(flat.paint, Paint::Stroke(_)) == stroked)
                .flat_map(|flat| flat.lines.iter().cloned())
                .collect()
        };
        let pen = common_width(&shapes).map_or(stroke, |width| stroke * (width / WEIGHT));
        Fit::new(&of(true), &of(false), size, pen)
    };
    place(&mut items, &fit);
    Tile {
        size,
        alpha: paint_items(&items, size, stroke).bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{Icon, IconStyle, weight};
    use super::super::svg::read;
    use super::*;

    fn tile(svg: &str, size: usize) -> Tile {
        let (drawing, problems) = read(svg);
        assert!(problems.is_empty(), "{problems:?}");
        render(&drawing, size, weight(size as f32))
    }

    fn at(tile: &Tile, x: usize, y: usize) -> u8 {
        tile.alpha[y * tile.size + x]
    }

    #[test]
    fn sue_s_own_svg_draws_sue_s_own_pixels() {
        for icon in Icon::ALL {
            for style in IconStyle::ALL {
                let (drawing, problems) = read(&super::super::super::svg(*icon, style));
                assert!(problems.is_empty(), "{}: {problems:?}", icon.name());
                for size in [16_usize, 24, 48] {
                    let stroke = weight(size as f32);
                    assert_eq!(
                        render(&drawing, size, stroke),
                        icon.tile(style, size, stroke),
                        "{} ({}) at {size}",
                        icon.name(),
                        style.name()
                    );
                }
            }
        }
    }

    #[test]
    fn even_and_odd_cuts_holes_whichever_way_round() {
        // Two squares running the same way: a hole by even and odd, a
        // solid block by the nonzero rule.
        let data = "M4 4 H20 V20 H4 Z M8 8 H16 V16 H8 Z";
        let hole = tile(
            &format!(r##"<svg viewBox="0 0 24 24"><path fill-rule="evenodd" d="{data}"/></svg>"##),
            24,
        );
        let solid = tile(
            &format!(r##"<svg viewBox="0 0 24 24"><path d="{data}"/></svg>"##),
            24,
        );
        assert_eq!(at(&hole, 12, 12), 0);
        assert_eq!(at(&hole, 6, 6), 255);
        assert_eq!(at(&solid, 12, 12), 255);
    }

    #[test]
    fn a_wider_pen_draws_wider() {
        let thin = tile(
            r##"<svg viewBox="0 0 24 24"><path d="M4 12 H20" stroke="#000" stroke-width="1.5"/></svg>"##,
            48,
        );
        let wide = tile(
            r##"<svg viewBox="0 0 24 24"><path d="M4 12 H20" stroke="#000" stroke-width="3"/></svg>"##,
            48,
        );
        let sum = |tile: &Tile| tile.alpha.iter().map(|a| u32::from(*a)).sum::<u32>();
        assert!(sum(&wide) > sum(&thin) * 3 / 2);
    }

    #[test]
    fn a_mask_hides_where_it_is_dark() {
        let cut = tile(
            r##"<svg viewBox="0 0 24 24"><mask id="m"><rect width="24" height="24" fill="#fff"/><rect x="10" y="0" width="4" height="24" fill="#000"/></mask>
                <rect x="2" y="10" width="20" height="4" mask="url(#m)"/></svg>"##,
            24,
        );
        assert_eq!(at(&cut, 5, 11), 255);
        assert_eq!(at(&cut, 12, 11), 0);
        // A mask with nothing light hides everything.
        let none = tile(
            r##"<svg viewBox="0 0 24 24"><mask id="m"/><rect x="2" y="10" width="20" height="4" mask="url(#m)"/></svg>"##,
            24,
        );
        assert!(none.alpha.iter().all(|a| *a == 0));
    }
}
