//! A sheet of every built-in brush painted by the engine alone, for a
//! person to look at while tuning the presets: the same strokes as the
//! canvas's sheet (slow, fast, light, heavy, a tap, a flick, a lift while
//! moving, a fast mouse stroke), on a clear layer over white and on a
//! layer of mid-gray. It needs no window, so it builds in seconds; the
//! canvas's own sheet (`every_brush_sheet` in the `sue` runtime tests)
//! is the one that shows what a person sees.
//!
//! `SUE_BRUSH_SHEET` names the folder it writes to (else
//! `sue-engine-sheet` in the temporary folder; one PPM picture per
//! brush and ground); `SUE_BRUSH_SHEET_ONLY` keeps to the brushes whose
//! ids hold it. Brushes with picture tips are left out (their pictures
//! live in the studio).

use sue_brush::input::{Device, Kind, Sample, Settings};
use sue_brush::paint::{Colors, PaperChoice, Resources};
use sue_brush::replay::{self, ENGINE, Recorder};
use sue_brush::texture::Paper;
use sue_core::{PixelFormat, RasterTile2D};
use sue_model::brush::TipSource;

/// One stroke: how the pen moves and presses each frame (points), how
/// many frames it rests before the lift, and whether it is a mouse.
struct Row {
    legs: Vec<([f32; 2], f32)>,
    rest: usize,
    mouse: bool,
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// The rows of the canvas's sheet, in its order.
fn rows() -> Vec<Row> {
    let pen = |legs: Vec<([f32; 2], f32)>, rest: usize| Row {
        legs,
        rest,
        mouse: false,
    };
    vec![
        pen(
            (0..40)
                .map(|i| {
                    let s = i as f32 / 39.0;
                    (
                        [3.5, 0.6 * (s * 6.0).sin()],
                        0.25 + 0.5 * ease(s.min(1.0 - s) * 2.0),
                    )
                })
                .collect(),
            0,
        ),
        pen(
            (0..9)
                .map(|i| {
                    (
                        [16.0, 2.0 - 0.5 * i as f32],
                        0.3 + 0.4 * ease(i as f32 / 8.0),
                    )
                })
                .collect(),
            0,
        ),
        pen((0..28).map(|_| ([5.0, 0.0], 0.12)).collect(), 0),
        pen((0..28).map(|_| ([5.0, 0.0], 0.95)).collect(), 0),
        pen(vec![([0.3, 0.0], 0.5)], 2),
        pen(
            (0..10)
                .map(|i| {
                    (
                        [4.0 + i as f32 * 2.5, -0.6 * i as f32],
                        0.55 - 0.04 * i as f32,
                    )
                })
                .collect(),
            0,
        ),
        pen((0..18).map(|_| ([7.5, 0.0], 0.6)).collect(), 0),
        Row {
            legs: (0..6)
                .map(|i| ([22.0, 3.0 - 1.5 * i as f32], 1.0))
                .collect(),
            rest: 0,
            mouse: true,
        },
    ]
}

/// The samples of `row` from `start`, four a frame for a pen, one for a
/// mouse, as the canvas's sheet sends them.
fn samples(row: &Row, start: [f32; 2]) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut time = 10.0;
    let press = |p: f32| (!row.mouse).then_some(p);
    let first = row.legs.first().map_or(0.5, |l| l.1);
    out.push(Sample {
        pressure: press(first),
        ..Sample::new(Kind::Down, start, time)
    });
    let mut at = start;
    let parts = if row.mouse { 1 } else { 4 };
    for (by, pressure) in &row.legs {
        for part in 1..=parts {
            let f = part as f32 / parts as f32;
            time += 0.016 / f64::from(parts);
            out.push(Sample {
                pressure: press(*pressure),
                ..Sample::new(Kind::Move, [at[0] + by[0] * f, at[1] + by[1] * f], time)
            });
        }
        at = [at[0] + by[0], at[1] + by[1]];
    }
    for _ in 0..row.rest {
        time += 0.016;
        out.push(Sample {
            pressure: press(first),
            ..Sample::new(Kind::Move, at, time)
        });
    }
    out.push(Sample::new(Kind::Up, at, time + 0.002));
    out
}

const W: u32 = 520;
const H: u32 = 220;

#[test]
#[ignore = "writes a sheet of every built-in brush for a person to look at"]
fn engine_sheet() {
    let folder =
        std::path::PathBuf::from(std::env::var_os("SUE_BRUSH_SHEET").unwrap_or_else(|| {
            std::env::temp_dir()
                .join("sue-engine-sheet")
                .into_os_string()
        }));
    std::fs::create_dir_all(&folder).unwrap();
    let only = std::env::var("SUE_BRUSH_SHEET_ONLY").ok();
    for preset in sue_brush::presets::builtin() {
        if matches!(preset.tip, TipSource::Images { .. }) {
            continue;
        }
        if only.as_deref().is_some_and(|o| !preset.id.contains(o)) {
            continue;
        }
        let behaviours = sue_brush::paint::Behaviours::from_preset(&preset);
        let paper = behaviours.texture.as_ref().and_then(|t| match &t.paper {
            PaperChoice::Builtin(name) => Paper::builtin(name),
            PaperChoice::Picture(_) => None,
        });
        let resources = Resources {
            paper: paper.map(std::sync::Arc::new),
            ..Resources::default()
        };
        for (ground, under) in [("white", [0_u8; 4]), ("gray", [128, 128, 128, 255])] {
            let back = if ground == "white" { 255.0 } else { 128.0 };
            let mut sheet: Vec<u8> = Vec::new();
            let mut rows_done = 0;
            for row in rows() {
                let mut raster = RasterTile2D::new(W, H, PixelFormat::Rgba8).unwrap();
                if preset.erase {
                    for p in raster.bytes_mut().unwrap().chunks_mut(4) {
                        p.copy_from_slice(&[0, 0, 0, 255]);
                    }
                } else {
                    for p in raster.bytes_mut().unwrap().chunks_mut(4) {
                        p.copy_from_slice(&under);
                    }
                }
                let device = if row.mouse {
                    Device::MOUSE
                } else {
                    Device::Pen
                };
                let mut settings = Settings::new(device);
                settings.seed = 7;
                let colors = Colors {
                    main: [0.0; 3],
                    sub: [1.0; 3],
                };
                let mut recorder = Recorder::new(ENGINE, [1; 32], &settings, colors, Vec::new());
                let width: f32 = row.legs.iter().map(|l| l.0[0]).sum();
                let start = [(W as f32 - width) * 0.5, H as f32 * 0.5];
                for s in samples(&row, start) {
                    recorder.take(s);
                }
                let kept = recorder.finish();
                replay::paint(
                    &kept,
                    &preset,
                    resources.clone(),
                    &mut raster,
                    [0, 0],
                    false,
                )
                .unwrap();
                for p in raster.bytes().unwrap().chunks(4) {
                    let a = f32::from(p[3]) / 255.0;
                    for &v in &p[..3] {
                        sheet.push((f32::from(v) * a + back * (1.0 - a)).round() as u8);
                    }
                }
                rows_done += 1;
            }
            let name = folder.join(format!("{}-{ground}.ppm", preset.id));
            let mut out = format!("P6\n{} {}\n255\n", W, H * rows_done).into_bytes();
            out.extend_from_slice(&sheet);
            std::fs::write(name, out).unwrap();
        }
    }
}

#[test]
#[ignore = "writes Sue's papers for a person to look at"]
fn papers() {
    let folder =
        std::path::PathBuf::from(std::env::var_os("SUE_BRUSH_SHEET").unwrap_or_else(|| {
            std::env::temp_dir()
                .join("sue-engine-sheet")
                .into_os_string()
        }));
    std::fs::create_dir_all(&folder).unwrap();
    for name in sue_brush::texture::BUILTIN {
        let paper = Paper::builtin(name).unwrap();
        let mut out = b"P5\n256 256\n255\n".to_vec();
        for y in 0..256 {
            for x in 0..256 {
                out.push((paper.sample(x as f32 + 0.5, y as f32 + 0.5, 1.0) * 255.0) as u8);
            }
        }
        std::fs::write(folder.join(format!("paper-{name}.pgm")), out).unwrap();
    }
}

/// The three mouse strokes of the hands-on check against Clip Studio
/// (P5-3), as the desktop driver sent them at 100% zoom: a slow stroke
/// of three moves a second apart that rests before the lift, a fast
/// drag, and a tap. Points are in the picture's pixels.
fn mouse_strokes() -> Vec<Vec<Sample>> {
    let at = |x: f32, y: f32| [(x - 582.0) * 1.76, (y - 340.0) * 1.76];
    let slow = [
        (600.0, 400.0),
        (640.0, 393.0),
        (690.0, 391.0),
        (740.0, 398.0),
    ];
    let mut strokes = Vec::new();
    let mut s = vec![Sample::new(Kind::Down, at(slow[0].0, slow[0].1), 10.0)];
    for (i, (x, y)) in slow.iter().enumerate().skip(1) {
        s.push(Sample::new(Kind::Move, at(*x, *y), 10.0 + i as f64));
    }
    s.push(Sample::new(Kind::Up, at(740.0, 398.0), 14.0));
    strokes.push(s);
    strokes.push(vec![
        Sample::new(Kind::Down, at(770.0, 400.0), 20.0),
        Sample::new(Kind::Move, at(880.0, 393.0), 20.05),
        Sample::new(Kind::Up, at(880.0, 393.0), 20.1),
    ]);
    strokes.push(vec![
        Sample::new(Kind::Down, at(905.0, 397.0), 30.0),
        Sample::new(Kind::Up, at(905.0, 397.0), 30.08),
    ]);
    strokes
}

/// Paints the three mouse strokes of the Clip Studio check with each
/// brush `SUE_BRUSH_MOUSE` names (`id=size` pairs split by commas, the
/// size Clip Studio's sub tool had), on white, into a PGM picture per
/// brush in `SUE_BRUSH_SHEET`'s folder.
#[test]
#[ignore = "writes the Sue side of the Clip Studio brush check"]
fn mouse_rows_for_the_clip_studio_check() {
    let Ok(list) = std::env::var("SUE_BRUSH_MOUSE") else {
        return;
    };
    let folder =
        std::path::PathBuf::from(std::env::var_os("SUE_BRUSH_SHEET").unwrap_or_else(|| {
            std::env::temp_dir()
                .join("sue-engine-sheet")
                .into_os_string()
        }));
    std::fs::create_dir_all(&folder).unwrap();
    let (w, h) = (600_u32, 211_u32);
    for pair in list.split(',') {
        let (id, size) = pair.split_once('=').unwrap();
        let mut preset = sue_brush::presets::builtin()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("no brush {id}"));
        preset.params.insert(
            sue_model::brush::param::SIZE.to_owned(),
            size.parse().unwrap(),
        );
        let behaviours = sue_brush::paint::Behaviours::from_preset(&preset);
        let paper = behaviours.texture.as_ref().and_then(|t| match &t.paper {
            PaperChoice::Builtin(name) => Paper::builtin(name),
            PaperChoice::Picture(_) => None,
        });
        let resources = Resources {
            paper: paper.map(std::sync::Arc::new),
            ..Resources::default()
        };
        let mut raster = RasterTile2D::new(w, h, PixelFormat::Rgba8).unwrap();
        for stroke in mouse_strokes() {
            let mut settings = Settings::new(Device::MOUSE);
            settings.seed = 7;
            let colors = Colors {
                main: [0.0; 3],
                sub: [1.0; 3],
            };
            let mut recorder = Recorder::new(ENGINE, [1; 32], &settings, colors, Vec::new());
            for s in stroke {
                recorder.take(s);
            }
            replay::paint(
                &recorder.finish(),
                &preset,
                resources.clone(),
                &mut raster,
                [0, 0],
                false,
            )
            .unwrap();
        }
        let mut out = format!("P5\n{w} {h}\n255\n").into_bytes();
        for p in raster.bytes().unwrap().chunks(4) {
            let a = f32::from(p[3]) / 255.0;
            out.push((f32::from(p[0]) * a + 255.0 * (1.0 - a)).round() as u8);
        }
        std::fs::write(folder.join(format!("{id}.pgm")), out).unwrap();
    }
}
