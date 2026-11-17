//! The Blend tool's blenders and the new erasers, read from the pixels
//! the engine lays: Blend, Painterly blender, Wet bleed blender, Textured
//! blender and Soothing watercolor each work the paint under them in a
//! way of their own (crash course CC-5, full guide 00:28:00), Tone
//! scraping takes a tone away in dots (full guide 00:27:43), and the
//! Multiple layers eraser is an eraser that says so.

use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::paint::{Behaviours, Colors, PaperChoice, Resources};
use sue_brush::replay::{self, ENGINE, Recorder};
use sue_brush::texture::Paper;
use sue_core::{PixelFormat, RasterTile2D};
use sue_model::BrushPreset;

const W: u32 = 320;
const H: u32 = 160;

fn preset(id: &str) -> BrushPreset {
    sue_brush::presets::find(id).unwrap_or_else(|| panic!("{id}"))
}

/// Black and white stripes 16 pixels wide, up and down.
fn stripes() -> RasterTile2D {
    let mut raster = RasterTile2D::new(W, H, PixelFormat::Rgba8).unwrap();
    for (i, p) in raster.bytes_mut().unwrap().chunks_mut(4).enumerate() {
        let v = if (i as u32 % W / 16).is_multiple_of(2) { 0 } else { 255 };
        p.copy_from_slice(&[v, v, v, 255]);
    }
    raster
}

/// `preset` dragged across `raster` along the middle from x 40 to 280,
/// pressing `pressure`, in red.
fn drag(preset: &BrushPreset, mut raster: RasterTile2D, pressure: f32) -> Vec<u8> {
    let behaviours = Behaviours::from_preset(preset);
    let paper = behaviours.texture.as_ref().and_then(|t| match &t.paper {
        PaperChoice::Builtin(name) => Paper::builtin(name),
        PaperChoice::Picture(_) => None,
    });
    let resources = Resources {
        paper: paper.map(std::sync::Arc::new),
        ..Resources::default()
    };
    let mut settings = Settings::new(Device::Pen);
    settings.seed = 11;
    let colors = Colors {
        main: [1.0, 0.0, 0.0],
        sub: [0.0; 3],
    };
    let mut recorder = Recorder::new(ENGINE, [3; 32], &settings, colors, Vec::new());
    let (x0, x1, y) = (40.0_f32, 280.0_f32, H as f32 * 0.5);
    let mut time = 2.0;
    recorder.take(Sample {
        pressure: Some(pressure),
        ..Sample::new(Kind::Down, [x0, y], time)
    });
    for i in 1..=100 {
        time += 0.005;
        let x = x0 + (x1 - x0) * i as f32 / 100.0;
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
    let path = std::path::Path::new(&folder).join(format!("blend-{name}.ppm"));
    std::fs::write(path, out).expect("the shot is written");
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// What the middle rows of the stroke's body look like: the mean of how
/// far each pixel moved from the stripe it was (0 to 255), the spread of
/// the reds across the stripes (high where stripes still show), and the
/// most red any pixel took on (a blender lays no red of its own).
fn look(pixels: &[u8]) -> (f32, f32, u8) {
    let before = stripes();
    let before = before.bytes().unwrap();
    let mid = H / 2;
    let (mut moved, mut count) = (0.0, 0.0);
    let mut values = Vec::new();
    let mut redness = 0_u8;
    for y in mid - 4..=mid + 4 {
        for x in 100..220 {
            let (a, b) = (at(pixels, x, y), at(before, x, y));
            moved += f32::from(a[0].abs_diff(b[0]));
            count += 1.0;
            values.push(f32::from(a[0]));
            redness = redness.max(a[0].saturating_sub(a[1]));
        }
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let spread =
        (values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32).sqrt();
    (moved / count, spread, redness)
}

const BLENDERS: [&str; 5] = [
    "sue.blend.blend",
    "sue.blend.painterly",
    "sue.blend.wet_bleed",
    "sue.blend.textured",
    "sue.blend.soothing_watercolor",
];

#[test]
fn every_blender_works_the_paint_under_it_its_own_way() {
    let mut looks = Vec::new();
    for id in BLENDERS {
        let pixels = drag(&preset(id), stripes(), 0.8);
        shot(id, &pixels);
        let (moved, spread, red) = look(&pixels);
        println!("{id}: moved {moved:.1}, spread {spread:.1}, red {red}");
        // It changes what it crosses...
        assert!(moved > 8.0, "{id} changes the stripes: {moved}");
        // ...with no paint of its own.
        assert!(red <= 2, "{id} lays no color of its own: {red}");
        looks.push((id, moved, spread));
    }
    // Five different behaviours: no two leave the stripes alike.
    for (i, a) in looks.iter().enumerate() {
        for b in &looks[i + 1..] {
            let differ = (a.1 - b.1).abs() + (a.2 - b.2).abs();
            assert!(differ > 3.0, "{} and {} look alike: {a:?} {b:?}", a.0, b.0);
        }
    }
    // Soothing watercolor is the gentlest wash and keeps the most of the
    // stripes; Painterly drags the paint furthest, so it moves the most;
    // Wet bleed lays thinly and bleeds less than Blend smooths.
    let by = |id: &str| looks.iter().find(|l| l.0 == id).copied().unwrap();
    let gentlest = looks.iter().map(|l| l.1).fold(f32::INFINITY, f32::min);
    assert_eq!(by("sue.blend.soothing_watercolor").1, gentlest);
    let most = looks.iter().map(|l| l.1).fold(0.0_f32, f32::max);
    assert_eq!(by("sue.blend.painterly").1, most);
    assert!(by("sue.blend.wet_bleed").1 < by("sue.blend.blend").1);
}

#[test]
fn the_textured_blender_is_broken_up_by_the_paper() {
    // Across the stripes, Blend leaves a smooth gray; the textured
    // blender's work is held back where the paper's valleys are, so its
    // gray is mottled: neighbouring pixels up and down the stroke differ
    // more.
    let plain = drag(&preset("sue.blend.blend"), stripes(), 0.8);
    let textured = drag(&preset("sue.blend.textured"), stripes(), 0.8);
    let roughness = |pixels: &[u8]| {
        let mid = H / 2;
        let mut sum = 0.0;
        for y in mid - 8..=mid + 8 {
            for x in 100..220 {
                sum += f32::from(at(pixels, x, y)[0].abs_diff(at(pixels, x, y + 1)[0]));
            }
        }
        sum
    };
    let (smooth, rough) = (roughness(&plain), roughness(&textured));
    println!("blend {smooth}, textured {rough}");
    assert!(rough > smooth * 1.3, "{rough} against {smooth}");
}

#[test]
fn tone_scraping_takes_a_tone_away_in_dots() {
    let mut raster = RasterTile2D::new(W, H, PixelFormat::Rgba8).unwrap();
    for p in raster.bytes_mut().unwrap().chunks_mut(4) {
        p.copy_from_slice(&[0, 0, 0, 255]);
    }
    let pixels = drag(&preset("sue.airbrush.tone_scraping"), raster, 1.0);
    shot("tone-scraping", &pixels);
    let mid = H / 2;
    let (mut gone, mut kept, mut between) = (0, 0, 0);
    for y in mid - 10..=mid + 10 {
        for x in 120..200 {
            match at(&pixels, x, y)[3] {
                0..=40 => gone += 1,
                215..=255 => kept += 1,
                _ => between += 1,
            }
        }
    }
    println!("gone {gone}, kept {kept}, between {between}");
    // Dots taken out of the tone, the tone kept between them: both many,
    // not a soft even fade.
    assert!(gone > 150, "dots scraped away: {gone}");
    assert!(kept > 150, "tone kept between them: {kept}");
    assert!(
        between < gone + kept,
        "a halftone, not a fade: {between} between"
    );
}

#[test]
fn the_new_erasers_say_what_they_do() {
    let multiple = preset("sue.eraser.multiple_layers");
    assert!(multiple.erase);
    assert!(Behaviours::from_preset(&multiple).all_layers);
    let snap = preset("sue.eraser.snap");
    assert!(snap.erase);
    assert_eq!(
        sue_brush::presets::vector_erase(&snap),
        1,
        "up to the crossing"
    );
    let order: Vec<String> = sue_brush::presets::of_tool("eraser")
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        &order[order.len() - 2..],
        ["Multiple layers", "Snap eraser"]
    );
}
