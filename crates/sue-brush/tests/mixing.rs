//! Mixing brushes really mix: white oil paint dragged out of black picks
//! up the black and smears it along, as a loaded brush does on a real
//! canvas (crash course tips, 28:14), and every mixing brush lays some
//! of what it passes over, not only its own color.

use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::paint::{Colors, PaperChoice, Resources};
use sue_brush::replay::{self, ENGINE, Recorder};
use sue_brush::texture::Paper;
use sue_core::{PixelFormat, RasterTile2D};
use sue_model::BrushPreset;

const W: u32 = 480;
const H: u32 = 160;

/// Where the black ends: the left of the layer is black, the right
/// clear.
const EDGE: u32 = 200;

fn preset(id: &str) -> BrushPreset {
    sue_brush::presets::find(id).unwrap_or_else(|| panic!("{id}"))
}

/// A stroke of `preset` in white from x 120 (in the black) to x 360 (in
/// the clear), pressing `pressure`, over a layer black left of [`EDGE`]
/// and clear right of it. The layer's pixels after the stroke.
fn drag_white_out_of_black(preset: &BrushPreset, pressure: f32) -> Vec<u8> {
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
    for (i, p) in raster.bytes_mut().unwrap().chunks_mut(4).enumerate() {
        if (i as u32 % W) < EDGE {
            p.copy_from_slice(&[0, 0, 0, 255]);
        }
    }
    let mut settings = Settings::new(Device::Pen);
    settings.seed = 3;
    let colors = Colors {
        main: [1.0; 3],
        sub: [0.0; 3],
    };
    let mut recorder = Recorder::new(ENGINE, [2; 32], &settings, colors, Vec::new());
    let (x0, x1, y) = (120.0_f32, 360.0_f32, H as f32 * 0.5);
    let mut time = 5.0;
    recorder.take(Sample {
        pressure: Some(pressure),
        ..Sample::new(Kind::Down, [x0, y], time)
    });
    let steps = 120;
    for i in 1..=steps {
        time += 0.004;
        let x = x0 + (x1 - x0) * i as f32 / steps as f32;
        recorder.take(Sample {
            pressure: Some(pressure),
            ..Sample::new(Kind::Move, [x, y], time)
        });
    }
    recorder.take(Sample::new(Kind::Up, [x1, y], time + 0.002));
    let kept = recorder.finish();
    replay::paint(&kept, preset, resources, &mut raster, [0, 0], false).unwrap();
    raster.bytes().unwrap().to_vec()
}

/// With `SUE_BRUSH_SHOTS` naming a folder, the layer over mid-gray as a
/// PPM picture there, for looking at by eye.
fn shot(name: &str, pixels: &[u8]) {
    let Some(folder) = std::env::var_os("SUE_BRUSH_SHOTS") else {
        return;
    };
    let mut out = format!(
        "P6
{W} {H}
255
"
    )
    .into_bytes();
    for p in pixels.chunks(4) {
        let a = f32::from(p[3]) / 255.0;
        for &v in &p[..3] {
            out.push((f32::from(v) * a + 128.0 * (1.0 - a)).round() as u8);
        }
    }
    let path = std::path::Path::new(&folder).join(format!("mix-{name}.ppm"));
    std::fs::write(path, out).expect("the shot is written");
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// The mean lightness (0 to 255) the stroke left on the middle rows of
/// columns `x0..x1`, over pixels it covers.
fn lightness(pixels: &[u8], x0: u32, x1: u32) -> f32 {
    let mid = H / 2;
    let mut sum = 0.0;
    let mut count = 0.0;
    for y in mid - 3..=mid + 3 {
        for x in x0..x1 {
            let p = at(pixels, x, y);
            if p[3] > 128 {
                sum += f32::from(p[0]);
                count += 1.0;
            }
        }
    }
    if count == 0.0 { -1.0 } else { sum / count }
}

#[test]
fn white_oil_dragged_out_of_black_smears_the_black_along() {
    for id in ["sue.brush.oil_paint", "sue.brush.thick_oil_paint"] {
        let pixels = drag_white_out_of_black(&preset(id), 0.8);
        shot(id, &pixels);
        // In the black, the brush does not cover it with clean white:
        // it mixes into a gray.
        let in_black = lightness(&pixels, 150, EDGE - 10);
        // Just past the edge the brush carries the black it took up: a
        // dark gray that lightens as its own white wins over the stroke.
        let past = lightness(&pixels, EDGE + 5, EDGE + 35);
        let further = lightness(&pixels, EDGE + 90, EDGE + 130);
        println!("{id}: in the black {in_black}, past the edge {past}, further {further}");
        assert!(
            in_black > 40.0 && in_black < 215.0,
            "{id}: a mix of black and white over the black: {in_black}"
        );
        assert!(
            past > 30.0 && past < 200.0,
            "{id}: the black picked up shows past the edge: {past}"
        );
        assert!(
            further > past + 15.0,
            "{id}: the smear lightens as it goes: {past} then {further}"
        );
        // Mixed pixels along the stroke, as the tutorial's test counts.
        let mixed = (EDGE..EDGE + 120)
            .filter(|&x| {
                let p = at(&pixels, x, H / 2);
                p[3] > 0 && p[0] > 30 && p[0] < 225
            })
            .count();
        assert!(mixed > 30, "{id}: {mixed} mixed pixels past the edge");
    }
}

#[test]
fn every_mixing_brush_mixes_with_what_it_crosses() {
    for preset in sue_brush::presets::builtin() {
        let Some(mix) = preset.mix else {
            continue;
        };
        // Scattered dots (pointillism) each land on their own place
        // and smear nothing along a way.
        if mix.amount <= 0.0
            || preset
                .value(sue_model::brush::param::SCATTER)
                .unwrap_or(0.0)
                > 0.0
        {
            continue;
        }
        let pixels = drag_white_out_of_black(&preset, 0.8);
        let in_black = lightness(&pixels, 150, EDGE - 10);
        println!("{}: in the black {in_black}", preset.id);
        // Pure white would be 255; nothing laid at all is -1.
        assert!(
            (0.0..240.0).contains(&in_black),
            "{}: lays only its own color over the black: {in_black}",
            preset.id
        );
    }
}
