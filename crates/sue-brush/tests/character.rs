//! Each built-in brush paints with the character its name promises, read
//! from the pixels the engine lays (not from its settings): a crayon
//! leaves specks of paper, a pencil has grain, gouache covers as fully as
//! the pen presses, a round watercolor is see-through with a darker edge,
//! a brush pen comes to a point at both ends with a mouse too, a felt pen
//! lays even, slightly see-through ink. And no brush's stroke ends are
//! bigger than its body.

use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::paint::{Colors, PaperChoice, Resources};
use sue_brush::replay::{self, ENGINE, Recorder};
use sue_brush::texture::Paper;
use sue_core::{PixelFormat, RasterTile2D};
use sue_model::BrushPreset;
use sue_model::brush::TipSource;

const W: u32 = 520;
const H: u32 = 220;

fn preset(id: &str) -> BrushPreset {
    sue_brush::presets::find(id).unwrap_or_else(|| panic!("{id}"))
}

/// A straight stroke from x 60 to x 300 along the middle, `frames`
/// frames at `pressure` (a mouse when `None`), on a layer filled with
/// `under`. The layer's pixels after the stroke.
fn paint(preset: &BrushPreset, pressure: Option<f32>, frames: usize, under: [u8; 4]) -> Vec<u8> {
    paint_from(preset, pressure, frames, under, [60.0, H as f32 * 0.5])
}

/// [`paint`], the stroke from `start` and 240 pixels long.
fn paint_from(
    preset: &BrushPreset,
    pressure: Option<f32>,
    frames: usize,
    under: [u8; 4],
    start: [f32; 2],
) -> Vec<u8> {
    let behaviours = sue_brush::paint::Behaviours::from_preset(preset);
    let paper = behaviours.texture.as_ref().and_then(|t| match &t.paper {
        PaperChoice::Builtin(name) => Paper::builtin(name),
        PaperChoice::Picture(_) => None,
    });
    let resources = Resources {
        paper: paper.map(std::sync::Arc::new),
        ..Resources::default()
    };
    let mut raster = RasterTile2D::new(W, H, PixelFormat::Rgba8).unwrap();
    for p in raster.bytes_mut().unwrap().chunks_mut(4) {
        p.copy_from_slice(&under);
    }
    let device = if pressure.is_some() {
        Device::Pen
    } else {
        Device::MOUSE
    };
    let mut settings = Settings::new(device);
    settings.seed = 7;
    let colors = Colors {
        main: [0.0; 3],
        sub: [1.0; 3],
    };
    let mut recorder = Recorder::new(ENGINE, [1; 32], &settings, colors, Vec::new());
    let [x0, y] = start;
    let x1 = x0 + 240.0;
    let mut time = 10.0;
    recorder.take(Sample {
        pressure,
        ..Sample::new(Kind::Down, [x0, y], time)
    });
    let parts = if pressure.is_some() { 4 } else { 1 };
    let steps = frames * parts;
    for i in 1..=steps {
        time += 0.016 / f64::from(parts as u32);
        let x = x0 + (x1 - x0) * i as f32 / steps as f32;
        recorder.take(Sample {
            pressure,
            ..Sample::new(Kind::Move, [x, y], time)
        });
    }
    recorder.take(Sample::new(Kind::Up, [x1, y], time + 0.002));
    let kept = recorder.finish();
    replay::paint(&kept, preset, resources, &mut raster, [0, 0], false).unwrap();
    raster.bytes().unwrap().to_vec()
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// How many rows of column `x` the stroke covers (alpha over 40).
fn height(pixels: &[u8], x: u32) -> u32 {
    (0..H).filter(|&y| at(pixels, x, y)[3] > 40).count() as u32
}

/// The alphas of the middle band (`half` rows about the middle) over the
/// stroke's body (x 120 to 240).
fn body(pixels: &[u8], half: u32) -> Vec<u8> {
    let mid = H / 2;
    (mid - half..=mid + half)
        .flat_map(|y| (120..240).map(move |x| (x, y)))
        .map(|(x, y)| at(pixels, x, y)[3])
        .collect()
}

#[test]
fn a_crayon_leaves_specks_of_paper_even_pressed_hard() {
    let pixels = paint(&preset("sue.pencil.crayon"), Some(0.95), 40, [0; 4]);
    let band = body(&pixels, 4);
    let specks = band.iter().filter(|&&a| a < 128).count();
    let solid = band.iter().filter(|&&a| a > 200).count();
    let share = specks as f32 / band.len() as f32;
    assert!(share > 0.04 && share < 0.6, "specks of paper: {share}");
    assert!(solid * 3 > band.len(), "a firm stroke is mostly wax");
    // A G-pen pressed as hard has none.
    let pen = paint(&preset("sue.pen.g_pen"), Some(0.95), 40, [0; 4]);
    assert!(body(&pen, 2).iter().all(|&a| a > 250));
}

#[test]
fn a_pencil_has_grain() {
    for id in ["sue.pencil.pencil", "sue.pencil.mechanical_pencil"] {
        let pixels = paint(&preset(id), Some(0.6), 40, [0; 4]);
        let band = body(&pixels, 1);
        let mean = band.iter().map(|&a| f32::from(a)).sum::<f32>() / band.len() as f32;
        let spread = (band
            .iter()
            .map(|&a| (f32::from(a) - mean).powi(2))
            .sum::<f32>()
            / band.len() as f32)
            .sqrt();
        assert!(mean > 40.0, "{id}: the line is there ({mean})");
        assert!(spread > 12.0, "{id}: graphite grain ({spread})");
    }
}

#[test]
fn gouache_covers_as_fully_as_the_pen_presses() {
    let gray = [128, 128, 128, 255];
    let g = preset("sue.brush.gouache");
    let mid = H / 2;
    let light = at(&paint(&g, Some(0.15), 40, gray), 180, mid);
    let firm = at(&paint(&g, Some(0.95), 40, gray), 180, mid);
    assert!(firm[0] < 25, "a firm stroke covers the gray: {firm:?}");
    assert!(
        light[0] > 50 && light[0] < 120,
        "a light one lays thin paint over it: {light:?}"
    );
    // Its first dabs, loaded before they pick any gray up, leave no
    // darker spot where the stroke began: thin paint is even along it.
    // (Where they land on the tile grid mattered: this start showed it.)
    let thin = paint_from(&g, Some(0.12), 48, gray, [190.0, 110.0]);
    let along: Vec<u8> = (176..260).map(|x| at(&thin, x, 110)[0]).collect();
    let (lo, hi) = (along.iter().min().unwrap(), along.iter().max().unwrap());
    assert!(hi - lo <= 1, "no spot at the start: {along:?}");
    // Over white paper too, a firm stroke is not see-through.
    let on_clear = at(&paint(&g, Some(0.95), 40, [0; 4]), 180, mid);
    assert!(on_clear[3] > 240, "{on_clear:?}");
}

#[test]
fn a_round_watercolor_is_see_through_with_a_darker_edge() {
    let pixels = paint(&preset("sue.brush.round_watercolor"), Some(0.7), 40, [0; 4]);
    let mid = H / 2;
    let x = 180;
    let inside = at(&pixels, x, mid)[3];
    let rows: Vec<u8> = (0..H).map(|y| at(&pixels, x, y)[3]).collect();
    let edge = rows.iter().copied().max().unwrap();
    assert!(inside > 60 && inside < 200, "see-through: {inside}");
    assert!(
        u32::from(edge) > u32::from(inside) + 25,
        "a darker rim: {edge} over {inside}"
    );
    // The darkest is near the edge of the stroke, not in its middle.
    let top = rows.iter().position(|&a| a > 20).unwrap() as u32;
    let darkest = rows.iter().position(|&a| a == edge).unwrap() as u32;
    let bottom = H - 1 - rows.iter().rev().position(|&a| a > 20).unwrap() as u32;
    let from_rim = (darkest - top).min(bottom - darkest);
    assert!(
        from_rim * 4 < bottom - top,
        "pooled at the rim: {top} {darkest} {bottom}"
    );
}

#[test]
fn a_brush_pen_comes_to_a_point_with_a_mouse_too() {
    let b = preset("sue.brush.brush_pen");
    for (what, pressure, frames) in [("a slow mouse", None, 60), ("a quick mouse", None, 12)] {
        let pixels = paint(&b, pressure, frames, [0; 4]);
        // A quick stroke's flick runs on past the lift: the ends are
        // where the paint ends, four pixels in.
        let covered: Vec<u32> = (0..W).filter(|&x| height(&pixels, x) > 0).collect();
        let (first, last) = (covered[0], covered[covered.len() - 1]);
        let middle = height(&pixels, 180);
        let start = height(&pixels, first + 4);
        let end = height(&pixels, last - 4);
        assert!(middle > 30, "{what}: the body ({middle})");
        assert!(
            start * 3 < middle && end * 3 < middle,
            "{what}: pointed ends {start} {middle} {end}"
        );
    }
}

#[test]
fn a_felt_pen_lays_even_slightly_see_through_ink() {
    let pixels = paint(&preset("sue.pen.felt_pen"), Some(0.6), 40, [0; 4]);
    let band = body(&pixels, 2);
    let (lo, hi) = (band.iter().min().unwrap(), band.iter().max().unwrap());
    assert!(*hi < 250, "not quite opaque: {hi}");
    assert!(hi - lo <= 3, "even: {lo} to {hi}");
}

/// Owner's bar: a stroke's ends are never bigger than its body, for any
/// built-in brush with a round or picture tip, pen or mouse.
#[test]
fn no_brush_ends_bigger_than_its_body() {
    for preset in sue_brush::presets::builtin() {
        if matches!(preset.tip, TipSource::Images { .. }) || preset.erase {
            continue;
        }
        for pressure in [Some(0.6), None] {
            let pixels = paint(&preset, pressure, 40, [0; 4]);
            let middle = (150..210).map(|x| height(&pixels, x)).max().unwrap();
            if middle == 0 {
                continue;
            }
            for x in (40..80).chain(280..320) {
                let h = height(&pixels, x);
                assert!(
                    h <= middle + 2,
                    "{} ({pressure:?}): {h} at x {x} over a body of {middle}",
                    preset.id
                );
            }
        }
    }
}
