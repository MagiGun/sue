//! The paint behaviours, stroke by stroke through the whole engine: pen
//! samples through the input pipeline, the dab plan, the buffers, the
//! finish and the lay onto a layer.
//!
//! `the_behaviour_sheet` draws one row for each behaviour and checks
//! each row against the plain pen it differs from; with
//! `SUE_BRUSH_SHOTS` set to a folder it writes every row there as a PNG,
//! to be looked at (`N:\Sue\plan\v0.0.8\shots\p3-7`). The rows, in
//! order, are listed in `ROWS`.

use std::sync::Arc;

use sue_brush::buffer::MergeError;
use sue_brush::input::{Device, Kind, Pipeline, RATE, Sample, Settings};
use sue_brush::paint::lay::EraseTarget;
use sue_brush::paint::overflow::Region;
use sue_brush::paint::wet::{BLENDERS, RasterBackdrop};
use sue_brush::paint::{
    Behaviours, Colors, DualBrush, DualMode, Flip, PaintStroke, PaperChoice, ParticleDirection,
    Resources, Spray, TextureChoice, TipOrder, Tone, ToneShape, WatercolorEdge,
};
use sue_brush::raster::TipImage;
use sue_brush::texture::{Paper, TextureMode, TextureSettings};
use sue_core::{PixelFormat, RasterTile2D};
use sue_model::brush::{AntiAlias, Binding, ColorJitter, Mixing, Spacing, param};
use sue_model::{BlendMode, BrushPreset};

/// The width and height of one sheet row.
const W: u32 = 1280;
const H: u32 = 150;

const BLACK: [f32; 3] = [0.08, 0.08, 0.1];

// ---------------------------------------------------------------- strokes

/// Pen samples along `path` (share 0 to 1 in, layer place out) over
/// `seconds`, pressing as `pressure` says, then the lift.
fn pen(path: impl Fn(f64) -> [f32; 2], seconds: f64, pressure: impl Fn(f64) -> f32) -> Vec<Sample> {
    let count = (seconds * 240.0).round().max(1.0) as usize;
    let mut out = Vec::new();
    for i in 0..=count {
        let share = i as f64 / count as f64;
        let kind = if i == 0 { Kind::Down } else { Kind::Move };
        out.push(Sample::new(kind, path(share), 1.0 + share * seconds).pressing(pressure(share)));
    }
    let last = *out.last().expect("samples");
    out.push(Sample::new(Kind::Up, last.pos, last.time + 1.0 / RATE));
    out
}

/// A wave from `x0` to `x1` around row height `y`.
fn wave(x0: f32, x1: f32, y: f32, amp: f32) -> impl Fn(f64) -> [f32; 2] {
    move |s| {
        let x = x0 + (x1 - x0) * s as f32;
        [x, y + amp * (s * std::f64::consts::TAU).sin() as f32]
    }
}

/// Light, heavy in the middle, light again.
fn swell(s: f64) -> f32 {
    (0.12 + 0.88 * (s * std::f64::consts::PI).sin().powf(0.8)) as f32
}

/// A preset of `size` whose size and opacity follow the pen a little.
fn pen_preset(size: f32, hardness: f32) -> BrushPreset {
    let mut preset = BrushPreset::new("Sheet");
    preset.set(param::SIZE, size);
    preset.set(param::HARDNESS, hardness);
    preset.antialias = AntiAlias::Medium;
    preset
        .dynamics
        .push(Binding::new(param::SIZE, sue_model::brush::input::PRESSURE).range(0.25, 1.0));
    preset
}

/// An airbrush: soft, building up.
fn airbrush(size: f32, flow: f32) -> BrushPreset {
    let mut preset = pen_preset(size, 0.0);
    preset.build_up = true;
    preset.spacing = Spacing::Ratio;
    preset.set(param::SPACING, 0.08);
    preset.set(param::FLOW, flow);
    preset.dynamics.clear();
    preset
        .dynamics
        .push(Binding::new(param::FLOW, sue_model::brush::input::PRESSURE).range(0.3, 1.0));
    preset
}

/// What one stroke is drawn with.
struct Brush {
    preset: BrushPreset,
    behaviours: Behaviours,
    resources: Resources,
    color: [f32; 3],
    keep_alpha: bool,
}

impl Brush {
    fn new(preset: BrushPreset) -> Self {
        Self {
            preset,
            behaviours: Behaviours::default(),
            resources: Resources::default(),
            color: BLACK,
            keep_alpha: false,
        }
    }

    fn with(mut self, behaviours: Behaviours) -> Self {
        self.behaviours = behaviours;
        self
    }

    fn resources(mut self, resources: Resources) -> Self {
        self.resources = resources;
        self
    }

    fn color(mut self, color: [f32; 3]) -> Self {
        self.color = color;
        self
    }
}

/// Runs `samples` through the pipeline and `brush`'s paint stroke, a
/// few samples a frame, drawing the tail every frame, holding the pen
/// still for `hold` seconds before the lift, and returns the stroke,
/// finished, not yet laid.
fn run(
    brush: &Brush,
    samples: &[Sample],
    seed: u64,
    hold: f64,
    layer: &RasterTile2D,
) -> PaintStroke {
    let registry = sue_brush::paint::registry();
    let mut settings = Settings::new(Device::Pen);
    settings.seed = seed;
    // The behaviours live on the preset, as a saved brush holds them;
    // the numbers a pen may drive reach the stroke through its points.
    let mut preset = brush.preset.clone();
    brush.behaviours.write_to(&mut preset);
    let mut pipeline = Pipeline::new(&preset, &registry, settings);
    let backdrop = RasterBackdrop {
        raster: layer,
        origin: [0, 0],
    };
    let mut stroke = PaintStroke::new(
        &preset,
        &Behaviours::from_preset(&preset),
        brush.resources.clone(),
        Colors {
            main: brush.color,
            sub: [0.95, 0.9, 0.2],
        },
        seed,
        pipeline.extra_keys(),
    );
    let mut done = 0;
    let (body, lift) = samples.split_at(samples.len() - 1);
    for frame in body.chunks(3) {
        pipeline.push_all(frame);
        let committed = pipeline.committed();
        stroke.commit(&committed[done..], Some(&backdrop));
        done = committed.len();
        stroke.tail(&pipeline.tail());
    }
    if hold > 0.0 {
        let end = body.last().expect("samples").time;
        let mut t = end;
        while t < end + hold {
            t += 1.0 / 60.0;
            stroke.hold(t, Some(&backdrop));
        }
    }
    pipeline.push_all(lift);
    let committed = pipeline.committed();
    stroke.commit(&committed[done..], Some(&backdrop));
    stroke.finish(Some(&backdrop));
    stroke
}

/// Draws `samples` with `brush` on `layer`.
fn draw(layer: &mut RasterTile2D, brush: &Brush, samples: &[Sample], seed: u64) {
    draw_held(layer, brush, samples, seed, 0.0);
}

/// [`draw`], holding the pen still for `hold` seconds at the end.
fn draw_held(
    layer: &mut RasterTile2D,
    brush: &Brush,
    samples: &[Sample],
    seed: u64,
    hold: f64,
) {
    let snapshot = layer.clone();
    let stroke = run(brush, samples, seed, hold, &snapshot);
    stroke.merge(layer, [0, 0], brush.keep_alpha).expect("laid");
}

// ----------------------------------------------------------------- layers

fn clear() -> RasterTile2D {
    RasterTile2D::new(W, H, PixelFormat::Rgba8).expect("a layer")
}

/// Fills the rectangle `x0..x1` by `y0..y1` of `layer` with `rgba`.
fn fill(layer: &mut RasterTile2D, x0: u32, x1: u32, y0: u32, y1: u32, rgba: [u8; 4]) {
    let bytes = layer.bytes_mut().expect("bytes");
    for y in y0..y1.min(H) {
        for x in x0..x1.min(W) {
            let i = ((y * W + x) * 4) as usize;
            bytes[i..i + 4].copy_from_slice(&rgba);
        }
    }
}

/// Upright stripes of six colors from `x0` to `x1`.
fn stripes(layer: &mut RasterTile2D, x0: u32, x1: u32) {
    const COLORS: [[u8; 4]; 6] = [
        [214, 48, 49, 255],
        [253, 203, 110, 255],
        [0, 148, 133, 255],
        [9, 132, 227, 255],
        [108, 92, 231, 255],
        [45, 52, 54, 255],
    ];
    let band = (x1 - x0) / COLORS.len() as u32;
    for (k, color) in COLORS.iter().enumerate() {
        let a = x0 + band * k as u32;
        fill(layer, a, a + band, 18, H - 18, *color);
    }
}

/// `layers` (bottom first) over warm white paper, as the sheet shows it.
fn flatten(layers: &[&RasterTile2D]) -> Vec<u8> {
    let mut out = vec![0_u8; (W * H * 3) as usize];
    for (i, px) in out.chunks_mut(3).enumerate() {
        let mut c = [250.0_f32, 248.0, 242.0];
        for layer in layers {
            let b = &layer.bytes().expect("bytes")[i * 4..i * 4 + 4];
            let a = f32::from(b[3]) / 255.0;
            for k in 0..3 {
                c[k] = f32::from(b[k]) * a + c[k] * (1.0 - a);
            }
        }
        for k in 0..3 {
            px[k] = (c[k] + 0.5) as u8;
        }
    }
    out
}

/// How many pixels of a flattened row differ from the paper.
fn inked(rgb: &[u8]) -> usize {
    rgb.chunks(3)
        .filter(|p| (i32::from(p[0]) - 250).abs() + (i32::from(p[2]) - 242).abs() > 12)
        .count()
}

/// How many pixels two flattened rows differ in.
fn differ(a: &[u8], b: &[u8]) -> usize {
    a.chunks(3)
        .zip(b.chunks(3))
        .filter(|(p, q)| p.iter().zip(*q).any(|(x, y)| x.abs_diff(*y) > 8))
        .count()
}

// --------------------------------------------------------------- pictures

/// A picture of `size` texels whose coverage at `(u, v)` (each -1 to 1)
/// is `f`.
fn picture(size: usize, f: impl Fn(f32, f32) -> f32) -> TipImage {
    let mut texels = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            texels.push(f(u, v).clamp(0.0, 1.0));
        }
    }
    TipImage::from_texels(size, size, texels).expect("a picture")
}

/// Sue's own sheet pictures, made by rule: a ragged blot, a leaf that
/// points one way (so flips show), and a spatter of drops.
fn pictures() -> Vec<TipImage> {
    let blot = picture(64, |u, v| {
        let r = (u * u + v * v).sqrt();
        let a = v.atan2(u);
        let edge = 0.78 + 0.12 * (a * 5.0).sin() + 0.06 * (a * 11.0 + 1.3).sin();
        ((edge - r) * 24.0).clamp(0.0, 1.0)
    });
    let leaf = picture(64, |u, v| {
        // A leaf along u, its tip at the right, curled up.
        let t = (u + 1.0) * 0.5;
        let half = 0.55 * (t * std::f32::consts::PI).sin() * (1.0 - 0.6 * t);
        let mid = 0.25 * t * t;
        ((half - (v + mid).abs()) * 30.0).clamp(0.0, 1.0)
    });
    let spatter = picture(64, |u, v| {
        // Five drops of paint of a few sizes.
        let drops = [
            (-0.45, -0.3, 0.32),
            (0.35, -0.45, 0.22),
            (0.4, 0.35, 0.3),
            (-0.3, 0.5, 0.18),
            (0.0, 0.0, 0.14),
        ];
        drops
            .iter()
            .map(|&(x, y, r): &(f32, f32, f32)| {
                let d = ((u - x) * (u - x) + (v - y) * (v - y)).sqrt();
                ((r - d) * 40.0).clamp(0.0, 1.0)
            })
            .fold(0.0, f32::max)
    });
    vec![blot, leaf, spatter]
}

/// A ring of reference lines for anti-overflow, around the middle of the
/// row's right part.
fn ring(layer: &mut RasterTile2D) {
    let bytes = layer.bytes_mut().expect("bytes");
    for y in 0..H {
        for x in 0..W {
            let (dx, dy) = ((x as f32 - 1000.0) / 200.0, (y as f32 - 75.0) / 55.0);
            let r = (dx * dx + dy * dy).sqrt();
            // Lines about three pixels thick, with a gap of none.
            if (r - 1.0).abs() < 3.0 / 110.0 {
                let i = ((y * W + x) * 4) as usize;
                bytes[i..i + 4].copy_from_slice(&[30, 30, 30, 255]);
            }
        }
    }
}

// ------------------------------------------------------------------- rows

/// The rows of the sheet, in order: (file name, what it shows).
const ROWS: &[(&str, &str)] = &[
    ("01-plain", "plain pens: hard, medium, soft (the reference)"),
    (
        "02-build-up",
        "a loop: without build-up, with it, four times the dabs, the same with density kept by gap",
    ),
    (
        "03-blend-mode",
        "over a band: normal, multiply, screen, overlay, color, then behind",
    ),
    (
        "04-texture-modes",
        "multiply grain, subtract rough, height grain, darken canvas, overlay cold-press",
    ),
    (
        "05-texture-settings",
        "canvas: plain, scale 2, turned 45, inverted, bright, contrast, emphasized",
    ),
    (
        "06-texture-by-dab",
        "an airbrush on grain, subtract: the texture on the stroke, then by each dab",
    ),
    (
        "07-color-jitter",
        "hue by dab, toward the sub color by dab, three strokes each its own hue",
    ),
    ("08-scatter", "scatter 0, 0.5 and 1.5 of the size"),
    (
        "09-spray",
        "spray: even, gathered, in pixels, picture particles turned at random",
    ),
    (
        "10-several-tips",
        "three pictures: repeat, back and forth, random with random flips",
    ),
    (
        "11-dual-brush",
        "dual: multiply round, subtract leaves, height spatter",
    ),
    (
        "12-watercolor-edge",
        "watercolor edge: none, narrow, wide and dark, made after the stroke",
    ),
    (
        "13-thickness-direction",
        "flat tip: fixed 45 degrees, along the line, a thin ellipse",
    ),
    ("14-sharp-angles", "a zigzag: sharp angles on, then off"),
    (
        "15-tone",
        "a tone airbrush laid, then a tone eraser scraping a dark band",
    ),
    (
        "16-erase-layers",
        "red and blue layers: one eraser stroke, the second on every layer",
    ),
    (
        "17-mixing",
        "over stripes: much paint, little paint with a long stretch, thin paint",
    ),
    (
        "18-blenders",
        "across stripe borders, high then low, left to right: blend, blur, finger tip, wet bleed, soothing watercolor, painterly, textured",
    ),
    (
        "19-anti-overflow",
        "a scribble without and with anti-overflow inside a ring of lines",
    ),
    (
        "20-gap",
        "the gap presets with the blot tip: narrow, normal, wide, very wide",
    ),
    (
        "21-continuous-spray",
        "a short spray held still for a second at its end, then one without",
    ),
    (
        "22-lock-alpha",
        "a red shape painted over with transparent pixels locked",
    ),
];

/// Draws row `name` and returns its flattened pixels.
#[allow(clippy::too_many_lines)]
fn row(name: &str) -> Vec<u8> {
    let tips = pictures();
    let grain = Paper::builtin("grain").expect("grain");
    let canvas = Paper::builtin("canvas").expect("canvas");
    let rough = Paper::builtin("rough").expect("rough");
    let cold = Paper::builtin("cold-press").expect("cold press");
    let mut layer = clear();
    let y = H as f32 * 0.5;
    let segment = |k: usize, of: usize| {
        let span = (W as f32 - 60.0) / of as f32;
        let x0 = 30.0 + span * k as f32 + 12.0;
        wave(x0, x0 + span - 24.0, y, 32.0)
    };
    let textured = |paper: PaperChoice, settings: TextureSettings| Behaviours {
        texture: Some(TextureChoice { paper, settings }),
        ..Behaviours::default()
    };
    let named = |n: &str| PaperChoice::Builtin(n.to_owned());
    match name {
        "01-plain" => {
            for (k, hardness) in [1.0, 0.6, 0.0].into_iter().enumerate() {
                draw(
                    &mut layer,
                    &Brush::new(pen_preset(26.0, hardness)),
                    &pen(segment(k, 3), 0.8, swell),
                    1,
                );
            }
        }
        "02-build-up" => {
            let loop_path = |x: f32| {
                move |s: f64| {
                    let a = s * std::f64::consts::TAU * 2.2;
                    [
                        x + 110.0 * s as f32 + 42.0 * a.cos() as f32 - 42.0,
                        y + 40.0 * a.sin() as f32,
                    ]
                }
            };
            // Without build-up the loop never darkens where it crosses
            // itself; with it, it does.
            let mut plain = airbrush(46.0, 0.25);
            plain.build_up = false;
            plain.set(param::OPACITY, 0.45);
            draw(
                &mut layer,
                &Brush::new(plain),
                &pen(loop_path(100.0), 1.2, |_| 0.8),
                2,
            );
            draw(
                &mut layer,
                &Brush::new(airbrush(46.0, 0.25)),
                &pen(loop_path(410.0), 1.2, |_| 0.8),
                2,
            );
            // Four times the dabs: darker, unless the density is kept by
            // the gap, when it matches the second loop.
            let mut close = airbrush(46.0, 0.25);
            close.set(param::SPACING, 0.02);
            draw(
                &mut layer,
                &Brush::new(close.clone()),
                &pen(loop_path(720.0), 1.2, |_| 0.8),
                2,
            );
            draw(
                &mut layer,
                &Brush::new(close).with(Behaviours {
                    gap_density: true,
                    ..Behaviours::default()
                }),
                &pen(loop_path(1030.0), 1.2, |_| 0.8),
                2,
            );
        }
        "03-blend-mode" => {
            // A band from warm to cold to paint across.
            let bytes = layer.bytes_mut().expect("bytes");
            for yy in 40..110 {
                for x in 0..W {
                    let t = x as f32 / W as f32;
                    let i = ((yy * W + x) * 4) as usize;
                    let c = [230.0 - 180.0 * t, 120.0 + 60.0 * t, 60.0 + 170.0 * t];
                    bytes[i..i + 3].copy_from_slice(&c.map(|v| v as u8));
                    // Holes in the band for "behind" to fill.
                    bytes[i + 3] = if x > 1080 && (x / 16 + yy / 16) % 2 == 0 {
                        0
                    } else {
                        255
                    };
                }
            }
            let modes = [
                (BlendMode::Normal, [0.5, 0.5, 0.5]),
                (BlendMode::Multiply, [0.5, 0.5, 0.5]),
                (BlendMode::Screen, [0.5, 0.5, 0.5]),
                (BlendMode::Overlay, [0.9, 0.6, 0.1]),
                (BlendMode::Color, [0.2, 0.6, 0.2]),
            ];
            for (k, (mode, color)) in modes.into_iter().enumerate() {
                let mut preset = pen_preset(36.0, 0.8);
                preset.blend = mode;
                draw(
                    &mut layer,
                    &Brush::new(preset).color(color),
                    &pen(segment(k, 6), 0.8, swell),
                    3,
                );
            }
            draw(
                &mut layer,
                &Brush::new(pen_preset(36.0, 0.8))
                    .color([0.1, 0.1, 0.4])
                    .with(Behaviours {
                        behind: true,
                        ..Behaviours::default()
                    }),
                &pen(segment(5, 6), 0.8, swell),
                3,
            );
        }
        "04-texture-modes" => {
            let papers = [
                (&grain, "grain"),
                (&rough, "rough"),
                (&grain, "grain"),
                (&canvas, "canvas"),
                (&cold, "cold-press"),
            ];
            for (k, mode) in [
                TextureMode::Multiply,
                TextureMode::Subtract,
                TextureMode::Height,
                TextureMode::Darken,
                TextureMode::Overlay,
            ]
            .into_iter()
            .enumerate()
            {
                let (paper, paper_name) = papers[k];
                let brush = Brush::new(pen_preset(40.0, 0.9))
                    .with(textured(
                        named(paper_name),
                        TextureSettings {
                            mode,
                            ..TextureSettings::default()
                        },
                    ))
                    .resources(Resources {
                        paper: Some(Arc::new(paper.clone())),
                        ..Resources::default()
                    });
                draw(&mut layer, &brush, &pen(segment(k, 5), 0.8, swell), 4);
            }
        }
        "05-texture-settings" => {
            let base = TextureSettings {
                mode: TextureMode::Subtract,
                strength: 0.8,
                ..TextureSettings::default()
            };
            let variants = [
                base,
                TextureSettings { scale: 2.0, ..base },
                TextureSettings {
                    rotation: 45.0,
                    ..base
                },
                TextureSettings {
                    invert: true,
                    ..base
                },
                TextureSettings {
                    brightness: 0.3,
                    ..base
                },
                TextureSettings {
                    contrast: 0.7,
                    ..base
                },
                TextureSettings {
                    emphasize: true,
                    strength: 0.45,
                    ..base
                },
            ];
            for (k, settings) in variants.into_iter().enumerate() {
                let brush = Brush::new(pen_preset(40.0, 0.9))
                    .with(textured(named("canvas"), settings))
                    .resources(Resources {
                        paper: Some(Arc::new(canvas.clone())),
                        ..Resources::default()
                    });
                draw(&mut layer, &brush, &pen(segment(k, 7), 0.8, swell), 5);
            }
        }
        "06-texture-by-dab" => {
            for (k, per_dab) in [false, true].into_iter().enumerate() {
                let brush = Brush::new(airbrush(60.0, 0.4))
                    .with(textured(
                        named("grain"),
                        TextureSettings {
                            mode: TextureMode::Subtract,
                            strength: 0.6,
                            per_dab,
                            ..TextureSettings::default()
                        },
                    ))
                    .resources(Resources {
                        paper: Some(Arc::new(grain.clone())),
                        ..Resources::default()
                    });
                let x0 = 60.0 + 620.0 * k as f32;
                let path = move |s: f64| {
                    let a = s * std::f64::consts::TAU * 2.0;
                    [x0 + 480.0 * s as f32, y + 30.0 * a.sin() as f32]
                };
                draw(&mut layer, &brush, &pen(path, 1.6, |_| 0.9), 6);
                draw(&mut layer, &brush, &pen(path, 1.6, |_| 0.9), 7);
            }
        }
        "07-color-jitter" => {
            let mut hue = pen_preset(30.0, 0.8);
            hue.spacing = Spacing::Ratio;
            hue.set(param::SPACING, 0.45);
            hue.color = ColorJitter {
                tip: true,
                hue: 0.25,
                saturation: 0.2,
                luminosity: 0.15,
                ..ColorJitter::default()
            };
            draw(
                &mut layer,
                &Brush::new(hue).color([0.85, 0.2, 0.25]),
                &pen(segment(0, 3), 0.8, swell),
                8,
            );
            let mut sub = pen_preset(30.0, 0.8);
            sub.spacing = Spacing::Ratio;
            sub.set(param::SPACING, 0.45);
            sub.color = ColorJitter {
                tip: true,
                sub: 1.0,
                ..ColorJitter::default()
            };
            draw(
                &mut layer,
                &Brush::new(sub).color([0.1, 0.3, 0.7]),
                &pen(segment(1, 3), 0.8, swell),
                9,
            );
            let mut stroke = pen_preset(22.0, 1.0);
            stroke.color = ColorJitter {
                stroke: true,
                stroke_hue: 0.5,
                ..ColorJitter::default()
            };
            for (k, seed) in [10_u64, 11, 12].into_iter().enumerate() {
                let x0 = 880.0 + 120.0 * k as f32;
                let path =
                    move |s: f64| [x0 + 60.0 * (s * 3.0).sin() as f32, 25.0 + 100.0 * s as f32];
                draw(
                    &mut layer,
                    &Brush::new(stroke.clone()).color([0.8, 0.3, 0.1]),
                    &pen(path, 0.6, swell),
                    seed,
                );
            }
        }
        "08-scatter" => {
            for (k, scatter) in [0.0, 0.5, 1.5].into_iter().enumerate() {
                let mut preset = pen_preset(14.0, 0.8);
                preset.spacing = Spacing::Ratio;
                preset.set(param::SPACING, 0.6);
                preset.set(param::SCATTER, scatter);
                draw(
                    &mut layer,
                    &Brush::new(preset),
                    &pen(segment(k, 3), 0.8, swell),
                    13,
                );
            }
        }
        "09-spray" => {
            let sprays = [
                Spray {
                    size: 0.05,
                    linked: true,
                    density: 18.0,
                    deviation: 0.0,
                    direction: ParticleDirection::Brush,
                },
                Spray {
                    size: 0.05,
                    linked: true,
                    density: 18.0,
                    deviation: 0.8,
                    direction: ParticleDirection::Brush,
                },
                Spray {
                    size: 2.0,
                    linked: false,
                    density: 24.0,
                    deviation: 0.3,
                    direction: ParticleDirection::Brush,
                },
                Spray {
                    size: 0.18,
                    linked: true,
                    density: 4.0,
                    deviation: 0.2,
                    direction: ParticleDirection::Random,
                },
            ];
            for (k, spray) in sprays.into_iter().enumerate() {
                let mut preset = pen_preset(70.0, 0.7);
                preset.dynamics.clear();
                let resources = if k == 3 {
                    Resources {
                        pictures: Arc::from(&tips[1..2]),
                        ..Resources::default()
                    }
                } else {
                    Resources::default()
                };
                let brush = Brush::new(preset)
                    .with(Behaviours {
                        spray: Some(spray),
                        ..Behaviours::default()
                    })
                    .resources(resources)
                    .color([0.15, 0.35, 0.25]);
                draw(&mut layer, &brush, &pen(segment(k, 4), 0.8, swell), 14);
            }
        }
        "10-several-tips" => {
            for (k, (order, flip)) in [
                (TipOrder::Repeat, Flip::Off),
                (TipOrder::BackAndForth, Flip::Off),
                (TipOrder::Random, Flip::Random),
            ]
            .into_iter()
            .enumerate()
            {
                let mut preset = pen_preset(40.0, 1.0);
                preset.spacing = Spacing::Ratio;
                preset.set(param::SPACING, 0.7);
                let brush = Brush::new(preset)
                    .with(Behaviours {
                        tip_order: order,
                        flip: [flip, Flip::Off],
                        ..Behaviours::default()
                    })
                    .resources(Resources {
                        pictures: Arc::from(&tips[..]),
                        ..Resources::default()
                    })
                    .color([0.2, 0.3, 0.15]);
                draw(&mut layer, &brush, &pen(segment(k, 3), 0.8, swell), 15);
            }
        }
        "11-dual-brush" => {
            let duals = [
                (DualMode::Multiply, 0.3, 1.2, &[][..]),
                (DualMode::Subtract, 0.45, 1.0, &tips[1..2]),
                (DualMode::Height, 0.6, 0.3, &tips[2..3]),
            ];
            for (k, (mode, size, scatter, pictures)) in duals.into_iter().enumerate() {
                let brush = Brush::new(pen_preset(48.0, 0.85))
                    .with(Behaviours {
                        dual: Some(DualBrush {
                            mode,
                            size,
                            linked: true,
                            spacing: 0.35,
                            scatter,
                            hardness: 0.5,
                            pictures: Vec::new(),
                        }),
                        ..Behaviours::default()
                    })
                    .resources(Resources {
                        dual_pictures: Arc::from(pictures),
                        ..Resources::default()
                    })
                    .color([0.35, 0.1, 0.3]);
                draw(&mut layer, &brush, &pen(segment(k, 3), 0.8, swell), 16);
            }
        }
        "12-watercolor-edge" => {
            let edges = [
                None,
                Some(WatercolorEdge {
                    range: 4.0,
                    ..WatercolorEdge::default()
                }),
                Some(WatercolorEdge {
                    range: 12.0,
                    opacity: 0.35,
                    darkness: 0.6,
                    blur: 1.0,
                    after_stroke: false,
                }),
                Some(WatercolorEdge {
                    after_stroke: true,
                    ..WatercolorEdge::default()
                }),
            ];
            for (k, edge) in edges.into_iter().enumerate() {
                let mut preset = pen_preset(56.0, 0.7);
                preset.set(param::OPACITY, 0.8);
                let brush = Brush::new(preset)
                    .with(Behaviours {
                        watercolor: edge,
                        ..Behaviours::default()
                    })
                    .color([0.2, 0.45, 0.75]);
                draw(&mut layer, &brush, &pen(segment(k, 4), 0.8, swell), 17);
            }
        }
        "13-thickness-direction" => {
            let mut fixed = pen_preset(40.0, 1.0);
            fixed.set(param::ASPECT, 0.2);
            fixed.set(param::ANGLE, 45.0);
            fixed.spacing = Spacing::Ratio;
            fixed.set(param::SPACING, 0.02);
            draw(
                &mut layer,
                &Brush::new(fixed.clone()),
                &pen(segment(0, 3), 0.8, swell),
                18,
            );
            // The tip turned with the line, a quarter turn across it. The
            // binding adds 0 to 360 to the base, which stays within the
            // angle's range (-360 to 360) from -90.
            let mut along = fixed.clone();
            along.set(param::ANGLE, -90.0);
            along.dynamics.push(
                Binding::new(param::ANGLE, sue_model::brush::input::DIRECTION)
                    .range(0.0, 360.0)
                    .added(),
            );
            draw(
                &mut layer,
                &Brush::new(along),
                &pen(segment(1, 3), 0.8, swell),
                18,
            );
            let mut thin = fixed;
            thin.set(param::ASPECT, 0.5);
            thin.set(param::ANGLE, 0.0);
            draw(
                &mut layer,
                &Brush::new(thin),
                &pen(segment(2, 3), 0.8, swell),
                18,
            );
        }
        "14-sharp-angles" => {
            for (k, sharp) in [true, false].into_iter().enumerate() {
                let x0 = 60.0 + 620.0 * k as f32;
                let zigzag = move |s: f64| {
                    let t = s as f32 * 6.0;
                    let up = (t.fract() - 0.5).abs() * 2.0;
                    [x0 + 520.0 * s as f32, 30.0 + 90.0 * up]
                };
                let brush = Brush::new(pen_preset(14.0, 1.0)).with(Behaviours {
                    sharp_angles: sharp,
                    ..Behaviours::default()
                });
                draw(&mut layer, &brush, &pen(zigzag, 1.2, |_| 0.9), 19);
            }
        }
        "15-tone" => {
            let tone = Tone {
                cell: 7.0,
                angle: 45.0,
                shape: ToneShape::Dot,
            };
            let brush = Brush::new(airbrush(90.0, 0.25)).with(Behaviours {
                tone: Some(tone),
                ..Behaviours::default()
            });
            draw(
                &mut layer,
                &brush,
                &pen(wave(60.0, 560.0, y, 10.0), 1.0, swell),
                20,
            );
            fill(&mut layer, 660, 1250, 25, 125, [20, 20, 30, 255]);
            let mut scraper = airbrush(80.0, 0.3);
            scraper.erase = true;
            let brush = Brush::new(scraper).with(Behaviours {
                tone: Some(Tone {
                    shape: ToneShape::Line,
                    angle: 30.0,
                    ..tone
                }),
                ..Behaviours::default()
            });
            draw(
                &mut layer,
                &brush,
                &pen(wave(700.0, 1210.0, y, 20.0), 1.0, swell),
                21,
            );
        }
        "18-blenders" => {
            // Seven narrow stripes; each blender crosses one border.
            stripes(&mut layer, 20, W - 20);
            let band = (W - 40) as f32 / 6.0;
            for (k, (name, _)) in BLENDERS.iter().enumerate() {
                let mut preset = pen_preset(30.0, 0.5);
                preset.dynamics.clear();
                preset.engine = sue_model::brush::EngineKind::Blend((*name).to_owned());
                // Borders at 20 + band * j; two blenders a border, one
                // high and one low.
                let border = 20.0 + band * (1 + k / 2) as f32;
                let row_y = if k % 2 == 0 { y - 28.0 } else { y + 28.0 };
                let path = move |s: f64| {
                    [
                        border - 70.0 + 140.0 * s as f32,
                        row_y + 10.0 * (s * 9.0).sin() as f32,
                    ]
                };
                let mut brush = Brush::new(preset);
                if *name == "textured" {
                    brush = brush
                        .with(textured(
                            named("canvas"),
                            TextureSettings {
                                per_dab: true,
                                strength: 0.9,
                                ..TextureSettings::default()
                            },
                        ))
                        .resources(Resources {
                            paper: Some(Arc::new(canvas.clone())),
                            ..Resources::default()
                        });
                }
                if *name == "painterly" {
                    brush = brush.resources(Resources {
                        pictures: Arc::from(&tips[0..1]),
                        ..Resources::default()
                    });
                }
                draw(&mut layer, &brush, &pen(path, 0.8, |_| 0.9), 22);
            }
        }
        "17-mixing" => {
            stripes(&mut layer, 20, W - 20);
            let mixes = [
                (
                    Mixing {
                        amount: 0.5,
                        load: 0.8,
                        pull: 0.2,
                        blur: 0.0,
                    },
                    1.0,
                    [0.95, 0.95, 0.9],
                ),
                (
                    Mixing {
                        amount: 0.6,
                        load: 0.15,
                        pull: 0.85,
                        blur: 0.0,
                    },
                    1.0,
                    [0.95, 0.95, 0.9],
                ),
                (
                    Mixing {
                        amount: 0.5,
                        load: 0.7,
                        pull: 0.3,
                        blur: 0.2,
                    },
                    0.35,
                    [0.95, 0.95, 0.9],
                ),
            ];
            for (k, (mixing, density, color)) in mixes.into_iter().enumerate() {
                let mut preset = pen_preset(40.0, 0.6);
                preset.mix = Some(mixing);
                let brush = Brush::new(preset)
                    .with(Behaviours {
                        paint_density: density,
                        ..Behaviours::default()
                    })
                    .color(color);
                draw(&mut layer, &brush, &pen(segment(k, 3), 0.8, |_| 0.9), 23);
            }
        }
        "19-anti-overflow" => {
            let mut lines = clear();
            ring(&mut lines);
            let region = Region::enclosing(&lines, [0, 0], [1000, 75], 0.5).expect("a region");
            let scribble = |x0: f32| {
                move |s: f64| {
                    let a = s * std::f64::consts::TAU * 5.0;
                    [x0 + 380.0 * s as f32, y + 70.0 * a.sin() as f32]
                }
            };
            // Without, over the ring at the left (drawn where a ring would be).
            let mut left = clear();
            let bytes = lines.bytes().expect("bytes").to_vec();
            {
                let l = left.bytes_mut().expect("bytes");
                for yy in 0..H {
                    for x in 0..W / 2 {
                        let from = ((yy * W + x + 640) * 4) as usize;
                        let to = ((yy * W + x) * 4) as usize;
                        l[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                    }
                }
            }
            let brush = Brush::new(pen_preset(30.0, 1.0)).color([0.95, 0.75, 0.2]);
            draw(&mut layer, &brush, &pen(scribble(170.0), 1.4, |_| 0.9), 24);
            let held = Brush::new(pen_preset(30.0, 1.0))
                .color([0.95, 0.75, 0.2])
                .resources(Resources {
                    overflow: Some(Arc::new(region.clone())),
                    ..Resources::default()
                });
            draw(&mut layer, &held, &pen(scribble(810.0), 1.4, |_| 0.9), 24);
            let mut both = left;
            {
                let b = both.bytes_mut().expect("bytes");
                for (i, px) in lines.bytes().expect("bytes").chunks(4).enumerate() {
                    if px[3] > 0 && i % W as usize >= 640 {
                        b[i * 4..i * 4 + 4].copy_from_slice(px);
                    }
                }
            }
            return flatten(&[&layer, &both]);
        }
        "20-gap" => {
            for (k, gap) in ["narrow", "normal", "wide", "very-wide"]
                .into_iter()
                .enumerate()
            {
                let mut preset = pen_preset(34.0, 1.0);
                preset.spacing = Spacing::Ratio;
                preset.set(
                    param::SPACING,
                    sue_brush::paint::plan::gap_preset(gap).expect("a gap"),
                );
                let brush = Brush::new(preset)
                    .resources(Resources {
                        pictures: Arc::from(&tips[0..1]),
                        ..Resources::default()
                    })
                    .color([0.3, 0.2, 0.1]);
                draw(&mut layer, &brush, &pen(segment(k, 4), 0.8, swell), 25);
            }
        }
        "21-continuous-spray" => {
            for (k, hold) in [1.0, 0.0].into_iter().enumerate() {
                let mut preset = pen_preset(80.0, 0.6);
                preset.dynamics.clear();
                preset.build_up = true;
                let brush = Brush::new(preset)
                    .with(Behaviours {
                        spray: Some(Spray {
                            size: 0.04,
                            linked: true,
                            density: 10.0,
                            deviation: 0.5,
                            direction: ParticleDirection::Brush,
                        }),
                        continuous: Some(120.0),
                        ..Behaviours::default()
                    })
                    .color([0.6, 0.1, 0.1]);
                let x0 = 100.0 + 620.0 * k as f32;
                draw_held(
                    &mut layer,
                    &brush,
                    &pen(wave(x0, x0 + 300.0, y, 15.0), 0.4, |_| 0.8),
                    26,
                    hold,
                );
            }
        }
        "22-lock-alpha" => {
            for k in 0..5 {
                fill(
                    &mut layer,
                    80 + 230 * k,
                    230 + 230 * k,
                    30,
                    120,
                    [210, 50, 50, 255],
                );
            }
            let mut brush = Brush::new(pen_preset(50.0, 0.5)).color([0.1, 0.1, 0.5]);
            brush.keep_alpha = true;
            draw(
                &mut layer,
                &brush,
                &pen(wave(40.0, 1240.0, y, 40.0), 1.2, swell),
                27,
            );
        }
        other => panic!("no row {other}"),
    }
    flatten(&[&layer])
}

/// The two layers of the multiple layers eraser row, flattened.
fn erase_row() -> Vec<u8> {
    let mut red = clear();
    let mut blue = clear();
    fill(&mut red, 40, W - 40, 20, 90, [200, 40, 40, 255]);
    fill(&mut blue, 40, W - 40, 60, 130, [40, 70, 200, 220]);
    let mut eraser = pen_preset(40.0, 0.8);
    eraser.erase = true;
    let brush = Brush::new(eraser);
    // One layer: only the blue.
    let snapshot = blue.clone();
    let stroke = run(
        &brush,
        &pen(wave(80.0, 560.0, 75.0, 35.0), 0.8, swell),
        28,
        0.0,
        &snapshot,
    );
    stroke.merge(&mut blue, [0, 0], false).expect("erased");
    // Every layer at once.
    let snapshot = red.clone();
    let stroke = run(
        &brush,
        &pen(wave(700.0, 1200.0, 75.0, 35.0), 0.8, swell),
        28,
        0.0,
        &snapshot,
    );
    let changed = stroke
        .erase_layers(&mut [
            EraseTarget {
                raster: &mut red,
                origin: [0, 0],
            },
            EraseTarget {
                raster: &mut blue,
                origin: [0, 0],
            },
        ])
        .expect("erased");
    assert!(changed.iter().all(Option::is_some));
    flatten(&[&red, &blue])
}

// ------------------------------------------------------------------ tests

/// `rgb` (one sheet row) as an uncompressed PNG: enough to look at, with
/// no dependency.
fn png(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * 3 + 1) as usize * height as usize);
    for row in rgb.chunks(width as usize * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
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
        out.extend_from_slice(&crc(&body).to_be_bytes());
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

fn crc(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn the_behaviour_sheet() {
    let plain = row("01-plain");
    assert!(inked(&plain) > 5_000, "the plain pens draw");
    let mut drawn = Vec::new();
    for (name, _) in ROWS {
        let rgb = if *name == "16-erase-layers" {
            erase_row()
        } else {
            row(name)
        };
        assert!(inked(&rgb) > 2_000, "{name} draws next to nothing");
        if *name != "01-plain" {
            assert!(
                differ(&rgb, &plain) > 2_000,
                "{name} looks like the plain pens"
            );
        }
        drawn.push((*name, rgb));
    }
    // The same row twice is the same pixels: every chance is seeded.
    assert_eq!(row("09-spray"), drawn[8].1);
    let Some(folder) = std::env::var_os("SUE_BRUSH_SHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).expect("the shots folder");
    let mut index = String::from("The paint behaviour sheet, one row a file:\n\n");
    for ((name, rgb), (_, what)) in drawn.iter().zip(ROWS) {
        std::fs::write(folder.join(format!("{name}.png")), png(rgb, W, H)).expect("a row");
        index.push_str(&format!("{name}: {what}\n"));
    }
    std::fs::write(folder.join("rows.txt"), index).expect("the index");
}

#[test]
fn a_stroke_shows_while_drawing_what_it_lays_on_lift() {
    // Everything but an edge made after the stroke: the live field of
    // the finished stroke is what the merge lays.
    let canvas = Paper::builtin("canvas").expect("canvas");
    let brush = Brush::new(pen_preset(40.0, 0.8))
        .with(Behaviours {
            texture: Some(TextureChoice {
                paper: PaperChoice::Builtin("canvas".into()),
                settings: TextureSettings::default(),
            }),
            watercolor: Some(WatercolorEdge::default()),
            tone: None,
            ..Behaviours::default()
        })
        .resources(Resources {
            paper: Some(Arc::new(canvas.clone())),
            ..Resources::default()
        });
    let layer = clear();
    let stroke = run(
        &brush,
        &pen(wave(40.0, 600.0, 75.0, 30.0), 0.8, swell),
        3,
        0.0,
        &layer,
    );
    let rect = stroke.bounds().expect("painted");
    let live = stroke.field(rect, true).expect("a field");
    let laid = stroke.field(rect, false).expect("a field");
    assert_eq!(live, laid);
}

#[test]
fn a_wet_stroke_mixes_what_it_lays_with_what_is_there() {
    let mut layer = clear();
    stripes(&mut layer, 20, W - 20);
    let before = layer.clone();
    let mut preset = pen_preset(40.0, 0.6);
    preset.mix = Some(Mixing {
        amount: 0.6,
        load: 0.2,
        pull: 0.8,
        blur: 0.0,
    });
    let brush = Brush::new(preset).color([1.0, 1.0, 1.0]);
    let snapshot = layer.clone();
    let stroke = run(
        &brush,
        &pen(wave(100.0, 900.0, 75.0, 0.0), 0.8, |_| 0.9),
        5,
        0.0,
        &snapshot,
    );
    assert!(stroke.is_wet());
    stroke.merge(&mut layer, [0, 0], false).expect("laid");
    // Across the stroke the stripes are dragged into each other: a
    // pixel just inside the third stripe holds the second's color.
    let px = |r: &RasterTile2D, x: u32| {
        let i = ((75 * W + x) * 4) as usize;
        let b = r.bytes().expect("bytes");
        [b[i], b[i + 1], b[i + 2]]
    };
    let band = (W - 40) / 6;
    let x = 20 + 2 * band + 6;
    assert_ne!(px(&layer, x), px(&before, x));
    let yellow = px(&before, x - 12);
    let after = px(&layer, x);
    assert!(
        after[0] > px(&before, x)[0] + 30,
        "{after:?} carries {yellow:?}"
    );
    // Far from the stroke, nothing moved.
    let i = ((20 * W + 400) * 4) as usize;
    assert_eq!(
        layer.bytes().expect("bytes")[i..i + 4],
        before.bytes().expect("bytes")[i..i + 4]
    );
}

#[test]
fn every_behaviour_reads_back_from_the_preset_it_was_written_to() {
    let behaviours = Behaviours {
        tip_order: TipOrder::Random,
        flip: [Flip::Random, Flip::On],
        spray: Some(Spray::default()),
        dual: Some(DualBrush::default()),
        watercolor: Some(WatercolorEdge::default()),
        tone: Some(Tone::default()),
        sharp_angles: false,
        ..Behaviours::default()
    };
    let mut preset = pen_preset(20.0, 1.0);
    behaviours.write_to(&mut preset);
    let bytes = preset.to_bytes();
    let read = BrushPreset::from_bytes(&bytes).expect("read");
    assert_eq!(Behaviours::from_preset(&read), behaviours);
}

#[test]
fn a_gray_layer_cannot_take_a_stroke() {
    let brush = Brush::new(pen_preset(20.0, 1.0));
    let layer = clear();
    let stroke = run(
        &brush,
        &pen(wave(10.0, 100.0, 50.0, 0.0), 0.3, swell),
        1,
        0.0,
        &layer,
    );
    let mut gray = RasterTile2D::new(64, 64, PixelFormat::Gray8).expect("a mask");
    assert_eq!(
        stroke.merge(&mut gray, [0, 0], false),
        Err(MergeError::Format(PixelFormat::Gray8))
    );
}

/// The alpha of layer pixel `(x, y)`.
fn alpha(layer: &RasterTile2D, x: u32, y: u32) -> u8 {
    layer.bytes().expect("bytes")[((y * W + x) * 4 + 3) as usize]
}

#[test]
fn build_up_darkens_where_a_stroke_crosses_itself_and_a_pen_does_not() {
    // A stroke there and back over the same line.
    let there_and_back = |s: f64| {
        let t = if s < 0.5 { s * 2.0 } else { 2.0 - s * 2.0 };
        [100.0 + 300.0 * t as f32, 75.0]
    };
    let samples = pen(there_and_back, 1.0, |_| 0.8);
    let once = pen(|s| [100.0 + 300.0 * s as f32, 75.0], 0.5, |_| 0.8);
    let mut plain = airbrush(30.0, 0.3);
    plain.build_up = false;
    plain.set(param::OPACITY, 0.4);
    let mut a = clear();
    draw(&mut a, &Brush::new(plain.clone()), &samples, 1);
    let mut b = clear();
    draw(&mut b, &Brush::new(plain), &once, 1);
    assert!(
        alpha(&a, 250, 75).abs_diff(alpha(&b, 250, 75)) <= 2,
        "a pen never stacks"
    );
    let mut c = clear();
    draw(&mut c, &Brush::new(airbrush(30.0, 0.3)), &samples, 1);
    let mut d = clear();
    draw(&mut d, &Brush::new(airbrush(30.0, 0.3)), &once, 1);
    assert!(
        alpha(&c, 250, 75) > alpha(&d, 250, 75) + 20,
        "build-up stacks"
    );
}

#[test]
fn a_flat_tip_turned_with_the_line_is_as_thick_whichever_way_it_runs() {
    let mut flat = pen_preset(30.0, 1.0);
    flat.dynamics.clear();
    flat.set(param::ASPECT, 0.2);
    flat.set(param::ANGLE, -90.0);
    flat.spacing = Spacing::Ratio;
    flat.set(param::SPACING, 0.02);
    flat.dynamics.push(
        Binding::new(param::ANGLE, sue_model::brush::input::DIRECTION)
            .range(0.0, 360.0)
            .added(),
    );
    let across = |layer: &RasterTile2D, x: u32, y: u32, horizontal: bool| {
        (0..60)
            .filter(|&k| {
                let (px, py) = if horizontal {
                    (x, y - 30 + k)
                } else {
                    (x - 30 + k, y)
                };
                alpha(layer, px, py) > 127
            })
            .count()
    };
    let mut right = clear();
    draw(
        &mut right,
        &Brush::new(flat.clone()),
        &pen(|s| [100.0 + 400.0 * s as f32, 75.0], 0.5, |_| 0.8),
        1,
    );
    let mut down = RasterTile2D::new(W, H, PixelFormat::Rgba8).expect("a layer");
    draw(
        &mut down,
        &Brush::new(flat),
        &pen(|s| [640.0, 10.0 + 130.0 * s as f32], 0.5, |_| 0.8),
        1,
    );
    let (w1, w2) = (across(&right, 300, 75, true), across(&down, 640, 75, false));
    // The tip lies across the line: its full width (30) shows, not its
    // thickness (6), either way.
    assert!(w1 > 24 && w2 > 24, "{w1} and {w2}");
    assert!(w1.abs_diff(w2) <= 1, "{w1} against {w2}");
}

#[test]
fn a_tone_eraser_scrapes_a_tone_away_dot_by_dot() {
    let mut layer = clear();
    fill(&mut layer, 0, W, 0, H, [0, 0, 0, 255]);
    let mut scraper = airbrush(80.0, 0.5);
    scraper.erase = true;
    let brush = Brush::new(scraper).with(Behaviours {
        tone: Some(Tone::default()),
        ..Behaviours::default()
    });
    draw(
        &mut layer,
        &brush,
        &pen(|s| [100.0 + 400.0 * s as f32, 75.0], 0.6, |_| 0.7),
        1,
    );
    // In the scraped band most pixels are either left whole or gone:
    // dots, not a gray.
    let (mut whole, mut gone, mut between) = (0, 0, 0);
    for y in 50..100 {
        for x in 150..450 {
            match alpha(&layer, x, y) {
                0..=20 => gone += 1,
                235..=255 => whole += 1,
                _ => between += 1,
            }
        }
    }
    assert!(gone > 1_000 && whole > 1_000, "{gone} gone, {whole} whole");
    assert!(between < (gone + whole) / 2, "{between} gray pixels");
}

#[test]
fn anti_overflow_keeps_a_stroke_inside_the_lines() {
    let mut lines = clear();
    ring(&mut lines);
    let region = Region::enclosing(&lines, [0, 0], [1000, 75], 0.5).expect("a region");
    let brush = Brush::new(pen_preset(30.0, 1.0)).resources(Resources {
        overflow: Some(Arc::new(region.clone())),
        ..Resources::default()
    });
    let mut layer = clear();
    draw(
        &mut layer,
        &brush,
        &pen(|s| [760.0 + 480.0 * s as f32, 75.0], 0.8, |_| 0.9),
        1,
    );
    // Inside the ring there is paint; outside it, none.
    assert!(alpha(&layer, 1000, 75) > 200);
    assert_eq!(alpha(&layer, 780, 75), 0);
    assert_eq!(alpha(&layer, 1225, 75), 0);
}

#[test]
fn brush_density_is_apart_from_opacity() {
    // Build-up dabs pile up to the stroke's opacity and never past it;
    // a low brush density builds up slowly, but on to full.
    let samples = pen(
        |s| {
            let t = (s * 8.0).fract();
            let back = (s * 8.0) as u32 % 2 == 1;
            let t = if back { 1.0 - t } else { t };
            [100.0 + 200.0 * t as f32, 75.0]
        },
        2.0,
        |_| 1.0,
    );
    let mut capped = airbrush(30.0, 1.0);
    capped.dynamics.clear();
    capped.set(param::OPACITY, 0.5);
    capped.set(param::DENSITY, 0.3);
    let mut layer = clear();
    draw(&mut layer, &Brush::new(capped), &samples, 1);
    let a = alpha(&layer, 200, 75);
    assert!((125..=129).contains(&a), "opacity caps the stroke: {a}");
    let mut thin = airbrush(30.0, 1.0);
    thin.dynamics.clear();
    thin.set(param::DENSITY, 0.05);
    let mut layer = clear();
    draw(
        &mut layer,
        &Brush::new(thin.clone()),
        &pen(|s| [100.0 + 200.0 * s as f32, 75.0], 0.3, |_| 1.0),
        1,
    );
    let once = alpha(&layer, 200, 75);
    let mut layer = clear();
    draw(&mut layer, &Brush::new(thin), &samples, 1);
    let many = alpha(&layer, 200, 75);
    assert!(
        once < 100 && many > 220,
        "{once} once, {many} after many passes"
    );
}

#[test]
fn a_wet_stroke_keeps_locked_clear_pixels_clear_and_stays_inside_the_lines() {
    // A finger tip from a red block out over clear pixels: locked, the
    // clear pixels stay clear and no pixel's alpha changes; unlocked,
    // the red is dragged out onto them.
    let mut preset = pen_preset(30.0, 0.6);
    preset.dynamics.clear();
    preset.engine = sue_model::brush::EngineKind::Blend("finger-tip".into());
    let samples = pen(|s| [180.0 + 200.0 * s as f32, 75.0], 0.5, |_| 0.9);
    let block = || {
        let mut layer = clear();
        fill(&mut layer, 100, 250, 50, 100, [200, 30, 30, 255]);
        layer
    };
    let mut locked = block();
    let mut brush = Brush::new(preset.clone());
    brush.keep_alpha = true;
    draw(&mut locked, &brush, &samples, 1);
    let mut free = block();
    draw(&mut free, &Brush::new(preset.clone()), &samples, 1);
    let original = block();
    for x in 100..400 {
        assert_eq!(alpha(&locked, x, 75), alpha(&original, x, 75), "x {x}");
    }
    assert!(alpha(&free, 300, 75) > 60, "{}", alpha(&free, 300, 75));

    // Inside a ring of lines, a mixing brush lays nothing outside it.
    let mut lines = clear();
    ring(&mut lines);
    let region = Region::enclosing(&lines, [0, 0], [1000, 75], 0.5).expect("a region");
    let mut layer = clear();
    stripes(&mut layer, 20, W - 20);
    let before = layer.clone();
    let mut wet = pen_preset(30.0, 0.6);
    wet.dynamics.clear();
    wet.mix = Some(Mixing {
        amount: 0.6,
        load: 0.8,
        pull: 0.2,
        blur: 0.0,
    });
    let brush = Brush::new(wet).color([1.0, 1.0, 1.0]).resources(Resources {
        overflow: Some(Arc::new(region.clone())),
        ..Resources::default()
    });
    draw(
        &mut layer,
        &brush,
        &pen(|s| [760.0 + 480.0 * s as f32, 75.0], 0.8, |_| 0.9),
        1,
    );
    let px = |r: &RasterTile2D, x: u32| {
        let i = ((75 * W + x) * 4) as usize;
        r.bytes().expect("bytes")[i..i + 4].to_vec()
    };
    assert_ne!(px(&layer, 1000), px(&before, 1000), "paint inside");
    assert_eq!(px(&layer, 780), px(&before, 780), "nothing outside");
    assert_eq!(px(&layer, 1225), px(&before, 1225), "nothing outside");
}
