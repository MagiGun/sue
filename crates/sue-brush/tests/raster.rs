//! The stroke buffer and the rasterizer primitives, held to what a
//! stroke must look like: opacity never stacks inside a stroke, sparse
//! samples leave no gap and no bead, a tap makes a dot, a picture tip
//! stamped small does not alias, and the work of an update is bounded by
//! the tail, however long the stroke.
//!
//! `sheets_of_every_primitive` also draws the primitives at several sizes
//! and hardnesses; with `SUE_BRUSH_SHOTS` set to a folder it writes them
//! there as PNG files, to be looked at.

use sue_brush::buffer::{Combine, StrokeBuffer, TILE};
use sue_brush::raster::kernel::{FULL, Ink};
use sue_brush::raster::{
    Dab, DabShape, DabWalker, Disc, Ribbon, RibbonPoint, RoundTip, Shape, TipImage, Turn,
};
use sue_core::{PixelFormat, RasterTile2D};

const BLACK: [f32; 3] = [0.0, 0.0, 0.0];

/// A white raster of `width` by `height`.
fn white(width: u32, height: u32) -> RasterTile2D {
    let mut raster = RasterTile2D::new(width, height, PixelFormat::Rgba8).expect("a raster");
    raster.bytes_mut().expect("bytes").fill(255);
    raster
}

/// The red sample of pixel `(x, y)`.
fn red(raster: &RasterTile2D, x: u32, y: u32) -> u8 {
    raster.bytes().expect("bytes")[((y * raster.width() + x) * 4) as usize]
}

/// The distance from `p` to the polyline through `points`.
fn distance_to_polyline(points: &[[f32; 2]], p: [f32; 2]) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let t =
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            let (ex, ey) = (p[0] - a[0] - dx * t, p[1] - a[1] - dy * t);
            (ex * ex + ey * ey).sqrt()
        })
        .fold(f32::INFINITY, f32::min)
}

/// `count` points on a circle of `radius` around `center`, closed.
fn circle(center: [f32; 2], radius: f32, count: usize) -> Vec<[f32; 2]> {
    (0..=count)
        .map(|i| {
            // A turn by a fixed step through a rotation, so no platform
            // `sin` decides where the samples are.
            let mut v = [radius, 0.0_f32];
            let (c, s) = step_turn(count);
            for _ in 0..i % count {
                v = [v[0] * c - v[1] * s, v[0] * s + v[1] * c];
            }
            [center[0] + v[0], center[1] + v[1]]
        })
        .collect()
}

/// The cosine and sine of a full turn over `count`, by their power
/// series, so the test needs no platform trigonometry.
fn step_turn(count: usize) -> (f32, f32) {
    let angle = 2.0 * std::f64::consts::PI / count as f64;
    let (mut c, mut s, mut term_c, mut term_s) = (0.0_f64, 0.0_f64, 1.0_f64, angle);
    for n in 0..12 {
        c += term_c;
        s += term_s;
        let k = 2 * n as u32 + 1;
        term_c *= -angle * angle / f64::from(k * (k + 1));
        term_s *= -angle * angle / f64::from((k + 1) * (k + 2));
    }
    (c as f32, s as f32)
}

#[test]
fn opacity_never_stacks_inside_a_stroke() {
    // A figure that crosses itself many times, drawn with dense samples
    // that overlap their neighbours almost entirely.
    let mut points = Vec::new();
    for i in 0..=400 {
        let t = i as f32 / 400.0;
        let x = 20.0 + 160.0 * t;
        // A zigzag that folds back over itself.
        let y = 100.0
            + if (i / 25) % 2 == 0 {
                (i % 25) as f32 * 3.0
            } else {
                (25 - i % 25) as f32 * 3.0
            }
            - 37.0;
        points.push(Disc::new([x, y], 9.0, 1.0));
    }
    let back: Vec<Disc> = points
        .iter()
        .rev()
        .map(|d| Disc::new([d.at[0], d.at[1] + 6.0], 9.0, 1.0))
        .collect();
    for tip in [RoundTip::hard(1.5), RoundTip::new(0.3, 1.0)] {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        buffer.committed().polyline(&points, tip);
        buffer.committed().polyline(&back, tip);
        // And dabs over the same ground, as a dab brush would add.
        let mut walker = DabWalker::new();
        for pair in points.windows(2) {
            walker.walk(
                pair[0],
                pair[1],
                |r| r * 0.1,
                |disc| buffer.committed().dab(&Dab::round(disc, tip)),
            );
        }
        let mut raster = white(200, 200);
        let ink = Ink::Paint {
            color: BLACK,
            opacity: 0.5,
        };
        buffer.merge(&mut raster, [0, 0], ink).expect("merged");
        let mut darkest = 255;
        for y in 0..200 {
            for x in 0..200 {
                darkest = darkest.min(red(&raster, x, y));
            }
        }
        // Black at half over white is 128 (127.5 rounded up), at most.
        assert_eq!(
            darkest, 128,
            "{tip:?}: a stroke at half opacity never goes past half"
        );
    }
}

#[test]
fn opacity_never_stacks_for_a_tail_drawn_again_and_again() {
    let tip = RoundTip::hard(0.9);
    let mut buffer = StrokeBuffer::new(Combine::Max);
    for step in 0..30 {
        let x = 10.0 + step as f32 * 3.0;
        buffer.committed().capsule(
            Disc::new([x, 30.0], 8.0, 1.0),
            Disc::new([x + 3.0, 30.0], 8.0, 1.0),
            tip,
        );
        buffer.tail().capsule(
            Disc::new([x + 3.0, 30.0], 8.0, 1.0),
            Disc::new([x + 20.0, 34.0], 8.0, 1.0),
            tip,
        );
    }
    buffer.settle();
    let mut raster = white(160, 60);
    buffer
        .merge(
            &mut raster,
            [0, 0],
            Ink::Paint {
                color: BLACK,
                opacity: 0.5,
            },
        )
        .expect("merged");
    let darkest = (0..60)
        .flat_map(|y| (0..160).map(move |x| (x, y)))
        .map(|(x, y)| red(&raster, x, y))
        .min();
    assert_eq!(darkest, Some(128));
}

#[test]
fn sparse_samples_lay_the_same_line_as_dense_ones() {
    // A straight line that thickens evenly: two samples or sixty, the
    // same pixels (the capsules of a chain share their discs exactly).
    let tip = RoundTip::hard(1.5);
    let (a, b) = (
        Disc::new([10.0, 40.0], 1.5, 1.0),
        Disc::new([230.0, 70.0], 14.0, 1.0),
    );
    let mut sparse = StrokeBuffer::new(Combine::Max);
    sparse.committed().polyline(&[a, b], tip);
    let dense_points: Vec<Disc> = (0..=60)
        .map(|i| {
            let t = i as f32 / 60.0;
            Disc::new(
                [
                    a.at[0] + (b.at[0] - a.at[0]) * t,
                    a.at[1] + (b.at[1] - a.at[1]) * t,
                ],
                a.radius + (b.radius - a.radius) * t,
                1.0,
            )
        })
        .collect();
    let mut dense = StrokeBuffer::new(Combine::Max);
    dense.committed().polyline(&dense_points, tip);
    let mut worst = 0;
    for y in 0..120 {
        for x in 0..260 {
            let diff = i32::from(sparse.coverage(x, y)) - i32::from(dense.coverage(x, y));
            worst = worst.max(diff.abs());
        }
    }
    assert!(worst <= 24, "sparse and dense differ by {worst} of {FULL}");
}

#[test]
fn sparse_samples_on_a_curve_leave_no_gap_and_no_bead() {
    // Twelve samples round a circle: far apart for a brush of radius 6.
    // Every pixel must get exactly the rim profile of its distance to the
    // polyline, so the line is as wide at a sample as between samples.
    let tip = RoundTip::hard(1.5);
    let ring = circle([100.0, 100.0], 60.0, 12);
    let discs: Vec<Disc> = ring.iter().map(|&at| Disc::new(at, 6.0, 1.0)).collect();
    let mut buffer = StrokeBuffer::new(Combine::Max);
    buffer.committed().polyline(&discs, tip);
    let mut worst = 0.0_f32;
    for y in 0..200 {
        for x in 0..200 {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let want = tip.hard_at(distance_to_polyline(&ring, p) - 6.0, 6.0);
            let got = f32::from(buffer.coverage(x, y)) / f32::from(FULL);
            worst = worst.max((want - got).abs());
        }
    }
    assert!(
        worst < 1.0e-3,
        "coverage strays {worst} from the exact band"
    );
}

#[test]
fn sparse_samples_of_a_soft_brush_leave_no_bead_at_the_joints() {
    // The profile across the line, measured at every sample and halfway
    // between: the same everywhere.
    let tip = RoundTip::new(0.2, 1.0);
    let ring = circle([100.0, 100.0], 70.0, 10);
    let discs: Vec<Disc> = ring.iter().map(|&at| Disc::new(at, 10.0, 1.0)).collect();
    let mut buffer = StrokeBuffer::new(Combine::Max);
    buffer.committed().polyline(&discs, tip);
    let across = |at: [f32; 2], normal: [f32; 2]| -> f32 {
        // The coverage summed along the normal, sampled at subpixel steps.
        (-60..=60)
            .map(|k| {
                let s = k as f32 * 0.25;
                let p = [at[0] + normal[0] * s, at[1] + normal[1] * s];
                let x = p[0].floor() as i32;
                let y = p[1].floor() as i32;
                f32::from(buffer.coverage(x, y)) / f32::from(FULL)
            })
            .sum()
    };
    let mut widths = Vec::new();
    for w in ring.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dy * dy).sqrt();
        let normal = [-dy / length, dx / length];
        let middle = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        widths.push(across(middle, normal));
    }
    let (low, high) = widths
        .iter()
        .fold((f32::INFINITY, 0.0_f32), |(l, h), &w| (l.min(w), h.max(w)));
    assert!(
        high - low < 0.06 * high,
        "widths between samples run from {low} to {high}"
    );
    // At a joint, across the bisector.
    for i in 1..ring.len() - 1 {
        let (p, q) = (ring[i - 1], ring[i + 1]);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let length = (dx * dx + dy * dy).sqrt();
        let joint = across(ring[i], [-dy / length, dx / length]);
        assert!(
            (joint - widths[0]).abs() < 0.06 * widths[0],
            "joint {i} is {joint} wide against {}",
            widths[0]
        );
    }
}

#[test]
fn a_tap_makes_a_dot() {
    for (radius, tip) in [
        (0.5_f32, RoundTip::hard(0.9)),
        (3.0, RoundTip::hard(0.9)),
        (12.0, RoundTip::new(0.5, 1.0)),
    ] {
        let center = [50.3, 40.7];
        let mut buffer = StrokeBuffer::new(Combine::Max);
        buffer
            .committed()
            .polyline(&[Disc::new(center, radius, 1.0)], tip);
        // The same dot from a zero-length segment and from one dab.
        let mut twice = StrokeBuffer::new(Combine::Max);
        twice.committed().capsule(
            Disc::new(center, radius, 1.0),
            Disc::new(center, radius, 1.0),
            tip,
        );
        let mut dab = StrokeBuffer::new(Combine::Max);
        let mut count = 0;
        DabWalker::new().walk(
            Disc::new(center, radius, 1.0),
            Disc::new(center, radius, 1.0),
            |r| r,
            |disc| {
                count += 1;
                dab.committed().dab(&Dab::round(disc, tip));
            },
        );
        assert_eq!(count, 1, "a tap is one dab");
        let (mut mass, mut mx, mut my) = (0.0_f64, 0.0_f64, 0.0_f64);
        for y in 0..100 {
            for x in 0..100 {
                let c = buffer.coverage(x, y);
                assert_eq!(c, twice.coverage(x, y));
                assert!(i32::from(c).abs_diff(i32::from(dab.coverage(x, y))) <= 1);
                let w = f64::from(c) / f64::from(FULL);
                mass += w;
                mx += w * (f64::from(x) + 0.5);
                my += w * (f64::from(y) + 0.5);
            }
        }
        assert!(mass > 0.0, "radius {radius}: a dot, not nothing");
        // Centered on the tap, to a tenth of a pixel.
        assert!(
            (mx / mass - f64::from(center[0])).abs() < 0.1,
            "radius {radius}: x {}",
            mx / mass
        );
        assert!(
            (my / mass - f64::from(center[1])).abs() < 0.1,
            "radius {radius}: y {}",
            my / mass
        );
        if tip.is_hard() && radius >= 1.0 {
            let area = std::f64::consts::PI * f64::from(radius) * f64::from(radius);
            assert!(
                (mass - area).abs() < 0.05 * area + 1.0,
                "radius {radius}: {mass} against {area}"
            );
        }
    }
}

/// A test picture: a ring with a bar sticking out to the right, so a
/// turn shows. Coverage worked out at 4 by 4 points a texel.
fn arrow(size: usize) -> TipImage {
    let mut coverage = Vec::with_capacity(size * size);
    let half = size as f32 * 0.5;
    for y in 0..size {
        for x in 0..size {
            let mut hit = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.0 - half) / half;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.0 - half) / half;
                    let r = (px * px + py * py).sqrt();
                    let ring = (0.55..0.85).contains(&r);
                    let bar = px > 0.0 && py.abs() < 0.12 && r < 0.98;
                    if ring || bar {
                        hit += 1;
                    }
                }
            }
            coverage.push((hit * 255 / 16) as u8);
        }
    }
    TipImage::from_coverage(size, size, &coverage).expect("the arrow")
}

/// One-texel checkers: the hardest picture to shrink.
fn checkers(size: usize) -> TipImage {
    let coverage: Vec<u8> = (0..size * size)
        .map(|i| {
            if (i % size + i / size).is_multiple_of(2) {
                255
            } else {
                0
            }
        })
        .collect();
    TipImage::from_coverage(size, size, &coverage).expect("checkers")
}

#[test]
fn a_picture_stamped_small_does_not_alias() {
    // 256 texels of one-texel checkers squeezed into 16 pixels: each
    // pixel holds 256 texels, whose mean is one half. Point sampling
    // would give 0 or 1 by where the pixel center happens to fall.
    let image = checkers(256);
    for shift in [0.0_f32, 0.13, 0.5, 0.77] {
        let dab = Dab::new(
            Disc::new([32.0 + shift, 32.0 + shift * 0.5], 8.0, 1.0),
            1.0,
            Turn::new(0.96, 0.28),
            DabShape::Image(&image),
        );
        // Pixels well inside the turned square.
        for y in 28..36 {
            for x in 28..36 {
                let c = dab.coverage([x as f32 + 0.5, y as f32 + 0.5]);
                assert!((c - 0.5).abs() < 0.03, "shift {shift} at ({x}, {y}): {c}");
            }
        }
    }
}

#[test]
fn a_dab_moves_by_subpixels_and_turns_with_its_turn() {
    let image = arrow(64);
    let mass_center = |dab: &Dab<'_>| {
        let (mut m, mut mx, mut my) = (0.0_f64, 0.0_f64, 0.0_f64);
        for y in 0..64 {
            for x in 0..64 {
                let c = f64::from(dab.coverage([x as f32 + 0.5, y as f32 + 0.5]));
                m += c;
                mx += c * (f64::from(x) + 0.5);
                my += c * (f64::from(y) + 0.5);
            }
        }
        [mx / m, my / m]
    };
    let at = |x: f32| {
        Dab::new(
            Disc::new([x, 30.0], 10.0, 1.0),
            1.0,
            Turn::NONE,
            DabShape::Image(&image),
        )
    };
    let round = |x: f32| Dab::round(Disc::new([x, 30.0], 4.0, 1.0), RoundTip::hard(0.9));
    let base = at(30.0);
    let (picture, disc) = (mass_center(&base), mass_center(&round(30.0)));
    let mut last = picture[0];
    for quarter in 1..8 {
        let shift = quarter as f32 * 0.125;
        let dab = at(30.0 + shift);
        // Nothing is snapped to the pixel grid: the paint is the same
        // paint, moved.
        for y in 10..50 {
            for x in 10..50 {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let moved = dab.coverage([p[0] + shift, p[1]]);
                assert!(
                    (moved - base.coverage(p)).abs() < 1.0e-5,
                    "{shift} at {p:?}"
                );
            }
        }
        // And what the pixels show moves with it, steadily. A picture
        // with hard edges read on a pixel grid wobbles by a few
        // hundredths of a pixel; a round dab, less.
        let moved = mass_center(&dab);
        assert!(moved[0] > last, "{shift}: the paint moves right");
        last = moved[0];
        assert!(
            ((moved[0] - picture[0]) - f64::from(shift)).abs() < 0.05,
            "{shift}: {moved:?} from {picture:?}"
        );
        assert!((moved[1] - picture[1]).abs() < 1.0e-6);
        let rounded = mass_center(&round(30.0 + shift));
        assert!(
            ((rounded[0] - disc[0]) - f64::from(shift)).abs() < 0.02,
            "{shift}: {rounded:?} from {disc:?}"
        );
    }
    // The bar points right; turned a quarter it points down.
    let flat = Dab::new(
        Disc::new([32.0, 32.0], 12.0, 1.0),
        1.0,
        Turn::NONE,
        DabShape::Image(&image),
    );
    let down = Dab::new(
        Disc::new([32.0, 32.0], 12.0, 1.0),
        1.0,
        Turn::new(0.0, 1.0),
        DabShape::Image(&image),
    );
    for y in 20..44 {
        for x in 20..44 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Turning (dx, dy) by a quarter makes (-dy, dx).
            let turned = down.coverage([32.0 - (py - 32.0), 32.0 + (px - 32.0)]);
            assert!(
                (flat.coverage([px, py]) - turned).abs() < 1.0e-4,
                "({x}, {y})"
            );
        }
    }
    assert!(flat.coverage([42.5, 32.5]) > 0.9);
    assert!(down.coverage([32.5, 42.5]) > 0.9);
}

#[test]
fn the_tail_costs_what_the_tail_costs_however_long_the_stroke() {
    // A long diagonal stroke: each update commits a few pixels and draws
    // a tail of four segments ahead. The work of an update must not grow
    // with what is already committed.
    let tip = RoundTip::hard(0.9);
    let mut buffer = StrokeBuffer::new(Combine::Max);
    let at = |i: usize| Disc::new([10.0 + i as f32 * 6.0, 10.0 + i as f32 * 4.0], 10.0, 1.0);
    let mut per_update = Vec::new();
    for i in 0..300 {
        buffer.reset_counters();
        buffer.committed().capsule(at(i), at(i + 1), tip);
        let mut tail = buffer.tail();
        for k in 1..5 {
            tail.capsule(at(i + k), at(i + k + 1), tip);
        }
        per_update.push(buffer.counters());
    }
    let early = per_update[10];
    for (i, c) in per_update.iter().enumerate().skip(10) {
        assert!(
            c.tail_pixels <= early.tail_pixels + early.tail_pixels / 10,
            "update {i}: {c:?} against {early:?}"
        );
        assert!(
            c.pixels <= early.pixels + early.pixels / 10,
            "update {i}: {c:?} against {early:?}"
        );
        // Four segments of tail lie in a handful of tiles.
        assert!(c.tail_tiles_cleared <= 6, "update {i}: {c:?}");
    }
    // The stroke by then spans hundreds of tiles' worth of length.
    let bounds = buffer.bounds().expect("bounds");
    assert!(bounds.x1 - bounds.x0 > 25 * TILE);
}

#[test]
fn build_up_dabs_add_up_and_a_pen_does_not() {
    let tip = RoundTip::new(0.5, 1.0);
    let disc = Disc::new([20.0, 20.0], 10.0, 0.3);
    let mut pen = StrokeBuffer::new(Combine::Max);
    let mut air = StrokeBuffer::new(Combine::BuildUp);
    for _ in 0..5 {
        pen.committed().dab(&Dab::round(disc, tip));
        air.committed().dab(&Dab::round(disc, tip));
    }
    let one = (0.3 * f32::from(FULL)).round() as u16;
    assert_eq!(pen.coverage(19, 19), one);
    // 1 - 0.7 ^ 5 of full.
    let built = f32::from(air.coverage(19, 19)) / f32::from(FULL);
    assert!((built - (1.0 - 0.7_f32.powi(5))).abs() < 0.002, "{built}");
}

#[test]
fn a_ribbon_is_continuous_along_a_bending_path() {
    // A full picture laid along a sparse curve: the band it covers has no
    // hole between the samples and none at the bends.
    let image = TipImage::from_coverage(8, 8, &[255; 64]).expect("a full picture");
    // Eight of nine steps round a circle: open, so its square ends lie
    // apart.
    let mut ring = circle([100.0, 100.0], 60.0, 9);
    ring.truncate(9);
    let mut points = Vec::new();
    let mut arc = 0.0;
    for (i, &at) in ring.iter().enumerate() {
        if i > 0 {
            let p = ring[i - 1];
            arc += ((at[0] - p[0]).powi(2) + (at[1] - p[1]).powi(2)).sqrt();
        }
        points.push(RibbonPoint {
            disc: Disc::new(at, 8.0, 1.0),
            arc,
        });
    }
    let ribbon = Ribbon::new(&points, &image, 12.0);
    let mut buffer = StrokeBuffer::new(Combine::Max);
    buffer.committed().ribbon(&ribbon);
    for y in 0..200 {
        for x in 0..200 {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let near_end = [ring[0], ring[8]]
                .iter()
                .any(|e| ((p[0] - e[0]).powi(2) + (p[1] - e[1]).powi(2)).sqrt() < 9.0);
            if distance_to_polyline(&ring, p) < 6.5 && !near_end {
                assert!(
                    buffer.coverage(x, y) > FULL - FULL / 50,
                    "a hole at ({x}, {y})"
                );
            }
            assert!(ribbon.coverage(p) <= 1.0);
        }
    }
}

// The sheets: every primitive at several sizes and hardnesses, to look at.

/// Draws one buffer black onto `raster`.
fn ink(buffer: &StrokeBuffer, raster: &mut RasterTile2D) {
    buffer
        .merge(
            raster,
            [0, 0],
            Ink::Paint {
                color: BLACK,
                opacity: 1.0,
            },
        )
        .expect("merged");
}

/// A wave from `(x, y)` `length` long, `samples` samples, its radius
/// going from thin to `radius` and back (a pen pressed and lifted).
fn wave(x: f32, y: f32, length: f32, radius: f32, samples: usize) -> Vec<Disc> {
    (0..=samples)
        .map(|i| {
            let t = i as f32 / samples as f32;
            // A smooth hump without trigonometry.
            let hump = 4.0 * t * (1.0 - t);
            let swing = (t * 2.0 - 1.0) * (t * 2.0 - 1.0) * (t * 2.0 - 1.0);
            Disc::new(
                [x + length * t, y + 10.0 * swing - 6.0 * hump],
                radius * (0.15 + 0.85 * hump),
                1.0,
            )
        })
        .collect()
}

fn sheet_capsules() -> RasterTile2D {
    let tips = [
        RoundTip::hard(0.0),
        RoundTip::hard(0.9),
        RoundTip::hard(1.5),
        RoundTip::hard(2.5),
        RoundTip::new(0.6, 1.0),
        RoundTip::new(0.0, 1.0),
    ];
    let sizes = [0.5_f32, 1.0, 1.5, 2.5, 4.0, 8.0, 16.0];
    let mut raster = white(820, 6 * 140);
    for (row, &tip) in tips.iter().enumerate() {
        let mut buffer = StrokeBuffer::new(Combine::Max);
        let mut x = 10.0;
        for &radius in &sizes {
            let width = 60.0 + radius * 6.0;
            // Sparse samples (6) and dense ones (60) side by side.
            buffer
                .committed()
                .polyline(&wave(x, 40.0 + row as f32 * 140.0, width, radius, 6), tip);
            buffer
                .committed()
                .polyline(&wave(x, 100.0 + row as f32 * 140.0, width, radius, 60), tip);
            x += width + 2.0 * radius + 14.0;
        }
        ink(&buffer, &mut raster);
    }
    raster
}

fn sheet_dabs() -> RasterTile2D {
    let mut raster = white(1180, 560);
    let arrow = arrow(128);
    let fine = checkers(256);
    let mut buffer = StrokeBuffer::new(Combine::Max);
    // Row 1: round dabs spaced along a line, hard and soft, sizes up.
    let mut x = 10.0;
    for (i, &radius) in [1.0_f32, 2.0, 4.0, 8.0, 14.0].iter().enumerate() {
        let tip = if i % 2 == 0 {
            RoundTip::hard(1.0)
        } else {
            RoundTip::new(0.3, 1.0)
        };
        let mut walker = DabWalker::new();
        let (a, b) = (
            Disc::new([x, 30.0], radius, 1.0),
            Disc::new([x + 180.0, 40.0], radius, 1.0),
        );
        walker.walk(
            a,
            b,
            |r| r * 1.2,
            |disc| buffer.committed().dab(&Dab::round(disc, tip)),
        );
        x += 230.0;
    }
    // Row 2: thin turned ellipses, a flat brush.
    let mut x = 20.0;
    for k in 0..16 {
        let turn = Turn::new(16.0 - k as f32, k as f32 * 1.4);
        buffer.committed().dab(&Dab::new(
            Disc::new([x, 100.0], 22.0, 1.0),
            0.18,
            turn,
            DabShape::Round(RoundTip::hard(1.0)),
        ));
        x += 64.0;
    }
    // Row 3: the arrow picture at sizes from 4 to 96, turning.
    let mut x = 10.0;
    for (k, &radius) in [2.0_f32, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0]
        .iter()
        .enumerate()
    {
        let turn = Turn::new(1.0, k as f32 * 0.3);
        buffer.committed().dab(&Dab::new(
            Disc::new([x + radius, 220.0], radius, 1.0),
            1.0,
            turn,
            DabShape::Image(&arrow),
        ));
        x += radius * 2.0 + 16.0;
    }
    // Row 4: subpixel placement: dabs of radius 1.5 a quarter pixel apart.
    for k in 0..40 {
        let tip = RoundTip::hard(0.9);
        buffer.committed().dab(&Dab::round(
            Disc::new([20.0 + k as f32 * 5.25, 310.0 + k as f32 * 0.25], 1.5, 1.0),
            tip,
        ));
    }
    // Row 5: checkers squeezed small: should read as even gray.
    let mut x = 20.0;
    for &radius in &[4.0_f32, 8.0, 16.0, 32.0, 64.0] {
        buffer.committed().dab(&Dab::new(
            Disc::new([x + radius, 460.0], radius, 1.0),
            1.0,
            Turn::new(0.9, 0.4),
            DabShape::Image(&fine),
        ));
        x += radius * 2.0 + 40.0;
    }
    ink(&buffer, &mut raster);
    raster
}

fn sheet_ribbons() -> RasterTile2D {
    let mut raster = white(1300, 420);
    let arrow = arrow(64);
    // Stripes across the line, fainter at their sides.
    let mut stripes = Vec::with_capacity(32 * 16);
    for y in 0..16 {
        for x in 0..32 {
            let along: u8 = if x < 20 { 255 } else { 0 };
            let across: u8 = if (2..14).contains(&y) { 255 } else { 60 };
            stripes.push(along.min(across));
        }
    }
    let stripes = TipImage::from_coverage(32, 16, &stripes).expect("stripes");
    let mut buffer = StrokeBuffer::new(Combine::Max);
    for (k, &radius) in [3.0_f32, 6.0, 12.0, 20.0].iter().enumerate() {
        let image = if k % 2 == 0 { &stripes } else { &arrow };
        // Most of a circle, in 14 samples: sharp bends for a ribbon.
        let mut path = circle([160.0 + k as f32 * 320.0, 210.0], 130.0, 16);
        path.truncate(14);
        let mut points = Vec::new();
        let mut arc = 0.0;
        for (i, &at) in path.iter().enumerate() {
            if i > 0 {
                let p = path[i - 1];
                arc += ((at[0] - p[0]).powi(2) + (at[1] - p[1]).powi(2)).sqrt();
            }
            points.push(RibbonPoint {
                disc: Disc::new(at, radius, 1.0),
                arc,
            });
        }
        buffer
            .committed()
            .ribbon(&Ribbon::new(&points, image, radius * 3.0));
    }
    ink(&buffer, &mut raster);
    raster
}

/// A part of `raster` blown up `scale` times, pixel for pixel.
fn zoom(raster: &RasterTile2D, x0: u32, y0: u32, w: u32, h: u32, scale: u32) -> RasterTile2D {
    let mut out = white(w * scale, h * scale);
    let src = raster.bytes().expect("bytes").to_vec();
    let width = raster.width();
    let bytes = out.bytes_mut().expect("bytes");
    for y in 0..h * scale {
        for x in 0..w * scale {
            let s = (((y0 + y / scale) * width + x0 + x / scale) * 4) as usize;
            let d = ((y * w * scale + x) * 4) as usize;
            bytes[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    out
}

#[test]
fn sheets_of_every_primitive() {
    let sheets = [
        ("capsules", sheet_capsules()),
        ("dabs", sheet_dabs()),
        ("ribbons", sheet_ribbons()),
    ];
    let zooms = [
        ("capsules-thin-zoom", zoom(&sheets[0].1, 0, 0, 300, 280, 4)),
        (
            "capsules-soft-zoom",
            zoom(&sheets[0].1, 0, 560, 300, 280, 4),
        ),
        ("dabs-small-zoom", zoom(&sheets[1].1, 0, 180, 260, 150, 6)),
        (
            "dabs-checkers-zoom",
            zoom(&sheets[1].1, 0, 380, 300, 160, 4),
        ),
        (
            "ribbons-bend-zoom",
            zoom(&sheets[2].1, 640, 60, 300, 300, 4),
        ),
    ];
    for (name, raster) in sheets.iter().chain(&zooms) {
        // Something was drawn on every sheet.
        assert!(
            raster.bytes().expect("bytes").iter().any(|&b| b < 128),
            "{name} is blank"
        );
    }
    let Some(folder) = std::env::var_os("SUE_BRUSH_SHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).expect("the shots folder");
    for (name, raster) in sheets.iter().chain(&zooms) {
        std::fs::write(folder.join(format!("{name}.png")), png(raster)).expect("a sheet written");
    }
}

/// `raster` as an uncompressed PNG (stored deflate blocks): enough to
/// look at, with no dependency.
fn png(raster: &RasterTile2D) -> Vec<u8> {
    let (width, height) = (raster.width(), raster.height());
    let bytes = raster.bytes().expect("bytes");
    let mut raw = Vec::with_capacity((width * 3 + 1) as usize * height as usize);
    for row in bytes.chunks(width as usize * 4) {
        raw.push(0);
        for px in row.chunks(4) {
            raw.extend_from_slice(&px[..3]);
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&sue_core::codec::crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

#[test]
fn a_thin_line_keeps_an_even_weight_along_its_length() {
    // A line at a slight slope crosses from one row of pixel centers to
    // the next. Summed down each column its ink must stay the same, or
    // the line ropes: lighter and darker beads where it passes between
    // pixel centers.
    for tip in [
        RoundTip::hard(0.9),
        RoundTip::hard(1.5),
        RoundTip::new(0.6, 1.0),
        RoundTip::new(0.0, 1.0),
    ] {
        for radius in [0.5_f32, 0.8, 1.2, 2.0, 4.0] {
            let mut buffer = StrokeBuffer::new(Combine::Max);
            buffer.committed().capsule(
                Disc::new([0.0, 20.1], radius, 1.0),
                Disc::new([200.0, 23.4], radius, 1.0),
                tip,
            );
            let sums: Vec<f32> = (20..180)
                .map(|x| {
                    (0..48)
                        .map(|y| f32::from(buffer.coverage(x, y)) / f32::from(FULL))
                        .sum()
                })
                .collect();
            let mean = sums.iter().sum::<f32>() / sums.len() as f32;
            let worst = sums.iter().map(|s| (s - mean).abs()).fold(0.0, f32::max);
            // Within one percent: what is left comes from the slope.
            assert!(
                worst < 0.01 * mean,
                "{tip:?} radius {radius}: {mean} off by {worst}"
            );
            // And the ink is the line's width (its strength once drawn
            // at least [`Disc::THINNEST`] wide).
            let drawn = Disc::new([0.0, 0.0], radius, 1.0).drawn(false);
            let width = if tip.is_hard() {
                2.0
            } else {
                1.0 + tip.hardness()
            } * drawn.radius
                * drawn.strength;
            assert!(
                (mean - width).abs() < 0.01 * width,
                "{tip:?} radius {radius}: {mean} against {width}"
            );
        }
    }
}
