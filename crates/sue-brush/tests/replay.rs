//! Replay: a stroke drawn live, kept, saved, read back and painted again
//! gives the same bytes; the engine's hash of a fixed stroke set is the
//! one committed for its version; no value that reaches a pixel goes
//! through the system's math library.
//!
//! The committed hash lives in `tests/golden/engine-hash.txt`, one line
//! per engine version. It is recorded on Windows now; Android compares
//! it in Phase 7 and the other platforms in Phase 8 of the 0.0.8 plan.
//! When it fails, the message says the hash this build made. Until the
//! 0.0.8 release only the brush steward records it again, after looking
//! at the stroke sheets; from the release on a change needs a new
//! engine version (`sue_brush::replay::ENGINE`).

use sue_brush::input::{Device, Kind, Pipeline, Sample, Settings};
use sue_brush::math;
use sue_brush::paint::wet::RasterBackdrop;
use sue_brush::paint::{
    Behaviours, Colors, DualBrush, PaintStroke, PaperChoice, Resources, Spray, TextureChoice,
    WatercolorEdge,
};
use sue_brush::raster::TipImage;
use sue_brush::replay::{self, ENGINE, Recorder, ReplayError, StoredStroke};
use sue_brush::texture::{Paper, TextureSettings};
use sue_core::{PixelFormat, RasterTile2D};
use sue_model::brush::{
    AntiAlias, Binding, ColorJitter, EngineKind, Mixing, Pick, PictureRef, Spacing, TipSource,
    input, param,
};
use sue_model::{BrushPreset, Response};

const W: u32 = 256;
const H: u32 = 96;

// ------------------------------------------------------------- the strokes

/// Pen samples along a wave from `x0` to `x1` around `y`, over `seconds`,
/// pressing lightly, then hard, then lightly, leaning and turning a
/// little, then the lift; the device's clock starts at `clock`. Made
/// with the engine's own math, so the samples are the same everywhere.
fn wave(x0: f32, x1: f32, y: f32, seconds: f64, clock: f64) -> Vec<Sample> {
    let count = (seconds * 200.0) as usize;
    let mut out = Vec::new();
    for i in 0..=count {
        let s = i as f32 / count as f32;
        let kind = if i == 0 { Kind::Down } else { Kind::Move };
        let mut sample = Sample::new(
            kind,
            [
                x0 + (x1 - x0) * s,
                y + 18.0 * math::sin(s * std::f32::consts::TAU * 1.5),
            ],
            clock + f64::from(s) * seconds,
        )
        .pressing(0.15 + 0.8 * math::sin(s * std::f32::consts::PI));
        sample.tilt = Some([0.3 * s, -0.2]);
        sample.twist = Some(40.0 * s);
        out.push(sample);
    }
    let last = *out.last().expect("samples");
    out.push(Sample::new(Kind::Up, last.pos, last.time + 0.004));
    out
}

/// The same samples moved by `by`.
fn moved(samples: &[Sample], by: [f32; 2]) -> Vec<Sample> {
    samples
        .iter()
        .map(|s| Sample {
            pos: [s.pos[0] + by[0], s.pos[1] + by[1]],
            ..*s
        })
        .collect()
}

/// A round pen of `size` whose size follows the pressure.
fn pen(size: f32) -> BrushPreset {
    let mut preset = BrushPreset::new("Replay pen");
    preset.set(param::SIZE, size);
    preset.antialias = AntiAlias::Medium;
    preset.taper.start = 12.0;
    preset.taper.end = 12.0;
    preset
        .dynamics
        .push(Binding::new(param::SIZE, input::PRESSURE).range(0.2, 1.0));
    preset
}

/// A picture of `size` by rule, with the engine's own math.
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

/// A ragged blot.
fn blot() -> TipImage {
    picture(48, |u, v| {
        let r = (u * u + v * v).sqrt();
        let a = math::atan2(v, u);
        let edge = 0.75 + 0.12 * math::sin(a * 5.0) + 0.06 * math::cos(a * 11.0 + 1.3);
        ((edge - r) * 20.0).clamp(0.0, 1.0)
    })
}

/// One stroke of the fixed set: its name, brush, behaviours, colors,
/// copies, what lies on the layer before it and how it is drawn.
struct Case {
    name: &'static str,
    preset: BrushPreset,
    behaviours: Behaviours,
    colors: Colors,
    copies: Vec<[f32; 6]>,
    device: Device,
    stabilization: u32,
    under: bool,
    pictures: bool,
    paper: bool,
}

impl Case {
    fn new(name: &'static str, preset: BrushPreset) -> Self {
        Self {
            name,
            preset,
            behaviours: Behaviours::default(),
            colors: Colors {
                main: [0.1, 0.12, 0.3],
                sub: [0.9, 0.7, 0.2],
            },
            copies: Vec::new(),
            device: Device::Pen,
            stabilization: 0,
            under: false,
            pictures: false,
            paper: false,
        }
    }

    /// The preset with the behaviours written onto it, as a saved brush
    /// holds them.
    fn brush(&self) -> BrushPreset {
        let mut preset = self.preset.clone();
        self.behaviours.write_to(&mut preset);
        preset.input.stabilization = self.stabilization;
        preset
    }
}

/// The fixed stroke set the engine's hash is taken over: one stroke for
/// each way the engine lays paint.
fn cases() -> Vec<Case> {
    let mut out = vec![Case::new("round pen", pen(14.0))];

    let mut soft = pen(30.0);
    soft.set(param::HARDNESS, 0.0);
    soft.build_up = true;
    soft.spacing = Spacing::Ratio;
    soft.set(param::SPACING, 0.1);
    soft.set(param::FLOW, 0.3);
    out.push(Case::new("airbrush", soft));

    let mut spray = pen(40.0);
    spray.spacing = Spacing::Ratio;
    spray.set(param::SPACING, 0.5);
    spray.set(param::SCATTER, 0.4);
    let mut case = Case::new("spray", spray);
    case.behaviours.spray = Some(Spray::default());
    out.push(case);

    let mut jitter = pen(16.0);
    jitter.color = ColorJitter {
        tip: true,
        hue: 0.3,
        saturation: 0.2,
        luminosity: 0.2,
        ..ColorJitter::default()
    };
    jitter.spacing = Spacing::Ratio;
    jitter.set(param::SPACING, 0.3);
    out.push(Case::new("color jitter", jitter));

    let mut stamp = pen(28.0);
    stamp.tip = TipSource::Images {
        pictures: vec![PictureRef::Hash([7; 32])],
        pick: Pick::Random,
        keep_colors: false,
    };
    stamp.spacing = Spacing::Ratio;
    stamp.set(param::SPACING, 0.6);
    stamp.dynamics.push(
        Binding::new(param::ANGLE, input::RANDOM)
            .range(0.0, 360.0)
            .added(),
    );
    let mut case = Case::new("picture tip", stamp);
    case.pictures = true;
    out.push(case);

    let mut wet = pen(26.0);
    wet.mix = Some(Mixing::new(0.6));
    let mut case = Case::new("mixing", wet);
    case.under = true;
    out.push(case);

    let mut finger = pen(24.0);
    finger.engine = EngineKind::Blend("finger-tip".to_owned());
    let mut case = Case::new("finger tip", finger);
    case.under = true;
    out.push(case);

    let mut case = Case::new("paper", pen(30.0));
    case.behaviours.texture = Some(TextureChoice {
        paper: PaperChoice::Builtin("grain".to_owned()),
        settings: TextureSettings::default(),
    });
    case.paper = true;
    out.push(case);

    let mut case = Case::new("watercolor edge", pen(34.0));
    case.behaviours.watercolor = Some(WatercolorEdge::default());
    out.push(case);

    let mut case = Case::new("dual brush", pen(30.0));
    case.behaviours.dual = Some(DualBrush::default());
    out.push(case);

    let mut case = Case::new("symmetry", pen(10.0));
    case.preset.set(param::OPACITY, 0.6);
    case.copies = vec![
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 1.0, W as f32, 0.0],
        [1.0, 0.0, 0.0, -1.0, 0.0, H as f32],
    ];
    out.push(case);

    let mut case = Case::new("stabilized mouse", pen(12.0));
    case.device = Device::MOUSE;
    case.stabilization = 40;
    out.push(case);

    let mut eraser = pen(20.0);
    eraser.erase = true;
    let mut case = Case::new("eraser", eraser);
    case.under = true;
    out.push(case);

    out
}

/// A clear layer, or one with bands of color to paint over.
fn layer(under: bool) -> RasterTile2D {
    let mut raster = RasterTile2D::new(W, H, PixelFormat::Rgba8).expect("a layer");
    if under {
        let bytes = raster.bytes_mut().expect("bytes");
        for y in 0..H {
            for x in 0..W {
                let i = ((y * W + x) * 4) as usize;
                let band = (x / 32) % 3;
                let rgb = [[200, 60, 40], [40, 160, 90], [230, 210, 60]][band as usize];
                bytes[i..i + 3].copy_from_slice(&rgb);
                bytes[i + 3] = if y > 20 && y < 76 { 255 } else { 0 };
            }
        }
    }
    raster
}

/// What a case reads besides its points.
struct Owned {
    pictures: Vec<TipImage>,
    dual: Vec<TipImage>,
    paper: Option<Paper>,
}

impl Owned {
    fn of(case: &Case) -> Self {
        Self {
            pictures: if case.pictures {
                vec![blot()]
            } else {
                Vec::new()
            },
            dual: Vec::new(),
            paper: case
                .paper
                .then(|| Paper::builtin("grain").expect("Sue's grain paper")),
        }
    }

    fn resources(&self) -> Resources {
        Resources {
            pictures: self.pictures.clone().into(),
            dual_pictures: self.dual.clone().into(),
            paper: self.paper.clone().map(std::sync::Arc::new),
            overflow: None,
        }
    }
}

/// Draws `samples` live, the way the worker does (see
/// `sue_brush::replay`): every sample through a recorder, a few a
/// frame, the tail drawn every frame, the stroke laid on `raster` on
/// the lift. Returns the stroke as kept.
fn draw_live(
    case: &Case,
    samples: &[Sample],
    frame: usize,
    raster: &mut RasterTile2D,
) -> StoredStroke {
    draw_frames(case, samples, frame, 0x5EED_0001, raster)
}

/// [`draw_live`] with chance from `seed`.
fn draw_live_seeded(
    case: &Case,
    samples: &[Sample],
    seed: u64,
    raster: &mut RasterTile2D,
) -> StoredStroke {
    draw_frames(case, samples, 3, seed, raster)
}

/// [`draw_live`], `frame` samples a frame, with chance from `seed`.
fn draw_frames(
    case: &Case,
    samples: &[Sample],
    frame: usize,
    seed: u64,
    raster: &mut RasterTile2D,
) -> StoredStroke {
    let preset = case.brush();
    let owned = Owned::of(case);
    let mut settings = Settings::new(case.device);
    settings.seed = seed;
    settings.scale = 1.5;
    // The device's samples are in screen points: twice the layer's.
    settings.to_layer = [[0.5, 0.0, 0.0], [0.0, 0.5, 0.0]];
    settings.curve = Response::new(&[[0.0, 0.0], [0.4, 0.6], [1.0, 1.0]]);
    let mut recorder = Recorder::new(ENGINE, [3; 32], &settings, case.colors, case.copies.clone());
    if !case.copies.is_empty() {
        // The copies of a symmetry ruler are laid by replay's own way of
        // laying them; the live view of those is the worker's (P3-6).
        for sample in samples {
            recorder.take(screen(sample));
        }
        let kept = recorder.finish();
        replay::paint(&kept, &preset, owned.resources(), raster, [0, 0], false).expect("laid");
        return kept;
    }
    let mut pipeline = Pipeline::new(&preset, &sue_brush::paint::registry(), recorder.settings());
    let mut stroke = PaintStroke::new(
        &preset,
        &Behaviours::from_preset(&preset),
        owned.resources(),
        case.colors,
        settings.seed,
        pipeline.extra_keys(),
    );
    let snapshot = raster.clone();
    let backdrop = RasterBackdrop {
        raster: &snapshot,
        origin: [0, 0],
    };
    let mut done = 0;
    for chunk in samples.chunks(frame) {
        for sample in chunk {
            let Some(kept) = recorder.take(screen(sample)) else {
                continue;
            };
            pipeline.push(kept);
            let committed = pipeline.committed();
            if committed.len() > done {
                stroke.commit(&committed[done..], Some(&backdrop));
                done = committed.len();
            }
            if kept.kind != Kind::Up {
                stroke.hold(kept.time, Some(&backdrop));
            }
        }
        // For show only; after the lift there is none to show.
        if !pipeline.is_done() {
            stroke.tail(&pipeline.tail());
        }
    }
    stroke.finish(Some(&backdrop));
    stroke.merge(raster, [0, 0], false).expect("laid");
    recorder.finish()
}

/// A sample given in layer pixels, as the device gave it in screen
/// points (twice as many).
fn screen(sample: &Sample) -> Sample {
    Sample {
        pos: [sample.pos[0] * 2.0, sample.pos[1] * 2.0],
        ..*sample
    }
}

/// The stored stroke painted again on a fresh layer like the first.
fn repaint(case: &Case, stroke: &StoredStroke) -> RasterTile2D {
    let owned = Owned::of(case);
    let mut raster = layer(case.under);
    replay::paint(
        stroke,
        &case.brush(),
        owned.resources(),
        &mut raster,
        [0, 0],
        false,
    )
    .expect("painted again");
    raster
}

/// FNV-1a, 64 bits.
fn fnv(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

const FNV: u64 = 0xcbf2_9ce4_8422_2325;

/// Every case drawn on its own layer, and the hash of all their bytes.
/// With `SUE_BRUSH_SHOTS` set to a folder, the set is written there as
/// `replay-set.png` (one row a case, over paper), to be looked at.
fn the_set() -> (u64, Vec<(&'static str, u64)>) {
    let mut all = FNV;
    let mut each = Vec::new();
    let mut sheet = Vec::new();
    for (k, case) in cases().iter().enumerate() {
        let samples = wave(24.0, 232.0, 48.0, 0.6, 1000.0 + k as f64);
        let mut raster = layer(case.under);
        draw_live(case, &samples, 3, &mut raster);
        let bytes = raster.bytes().expect("bytes");
        all = fnv(bytes, all);
        each.push((case.name, fnv(bytes, FNV)));
        for pixel in bytes.chunks(4) {
            let a = f32::from(pixel[3]) / 255.0;
            for (c, paper) in pixel[..3].iter().zip([250.0_f32, 248.0, 242.0]) {
                sheet.push((f32::from(*c) * a + paper * (1.0 - a) + 0.5) as u8);
            }
        }
    }
    if let Some(dir) = std::env::var_os("SUE_BRUSH_SHOTS") {
        let rows = (sheet.len() / (W as usize * 3)) as u32;
        std::fs::write(
            std::path::Path::new(&dir).join("replay-set.png"),
            png(&sheet, W, rows),
        )
        .expect("the sheet is written");
    }
    (all, each)
}

/// `rgb` as an uncompressed PNG, `width` by `height`.
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

// ------------------------------------------------------------------- tests

#[test]
fn a_stroke_drawn_kept_saved_and_read_back_paints_the_same_bytes() {
    for (k, case) in cases().iter().enumerate() {
        let samples = wave(24.0, 232.0, 48.0, 0.6, 5000.0 + k as f64);
        let mut live = layer(case.under);
        let kept = draw_live(case, &samples, 3, &mut live);
        assert!(kept.is_finished(), "{}", case.name);
        let read = StoredStroke::from_bytes(&kept.to_bytes()).expect("the stroke reads");
        assert_eq!(read, kept, "{}: the bytes keep everything", case.name);
        let again = repaint(case, &read);
        assert_eq!(
            again.bytes(),
            live.bytes(),
            "{}: painted again, the same bytes",
            case.name
        );
        assert_ne!(
            again.bytes(),
            layer(case.under).bytes(),
            "{} paints",
            case.name
        );
    }
}

#[test]
fn how_the_samples_came_in_frames_does_not_change_the_bytes() {
    let case = &cases()[0];
    let samples = wave(24.0, 232.0, 48.0, 0.6, 3.0);
    let mut one = layer(false);
    draw_live(case, &samples, 1, &mut one);
    for frame in [2, 5, 8, 40] {
        let mut other = layer(false);
        draw_live(case, &samples, frame, &mut other);
        assert_eq!(other.bytes(), one.bytes(), "{frame} samples a frame");
    }
}

#[test]
fn a_stroke_moved_elsewhere_keeps_its_look() {
    // Chance comes from the stored seed, never from where the stroke
    // lies: the spray moved by whole pixels lays its particles in the
    // same places, moved (to within the rounding of the moved places),
    // while another seed lays them elsewhere.
    let spray = cases()
        .into_iter()
        .find(|case| case.name == "spray")
        .expect("the spray");
    let samples = wave(24.0, 150.0, 40.0, 0.4, 2.0);
    let mut here = layer(false);
    draw_live(&spray, &samples, 3, &mut here);
    let mut there = layer(false);
    draw_live(&spray, &moved(&samples, [32.0, 8.0]), 3, &mut there);
    let (a, b) = (here.bytes().expect("bytes"), there.bytes().expect("bytes"));
    let gap = |a: &[u8], b: &[u8]| -> (u8, usize) {
        let (mut most, mut inked) = (0, 0);
        for y in 0..H - 8 {
            for x in 0..W - 32 {
                let i = ((y * W + x) * 4) as usize;
                let j = (((y + 8) * W + x + 32) * 4) as usize;
                for c in 0..4 {
                    most = most.max(a[i + c].abs_diff(b[j + c]));
                }
                inked += usize::from(a[i + 3] > 0);
            }
        }
        (most, inked)
    };
    let (most, inked) = gap(a, b);
    assert!(inked > 500, "the spray painted: {inked}");
    assert!(most <= 2, "moved, the same particles: off by {most}");
    // With another seed the particles fall elsewhere.
    let mut other = layer(false);
    draw_live_seeded(&spray, &moved(&samples, [32.0, 8.0]), 7, &mut other);
    let (most, _) = gap(a, other.bytes().expect("bytes"));
    assert!(most > 100, "another seed, other particles: off by {most}");
}

#[test]
fn the_engine_paints_the_same_on_every_thread() {
    let (here, _) = the_set();
    let others: Vec<u64> = (0..4)
        .map(|_| std::thread::spawn(|| the_set().0))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|handle| handle.join().expect("the thread finished"))
        .collect();
    assert!(
        others.iter().all(|hash| *hash == here),
        "{others:x?} {here:x}"
    );
}

#[test]
fn the_engine_hash_is_the_one_committed_for_its_version() {
    let (hash, each) = the_set();
    let committed = include_str!("golden/engine-hash.txt");
    let line = committed
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("engine") && words.next() == Some(&ENGINE.to_string()))
                .then(|| words.next().map(str::to_owned))
                .flatten()
        });
    let each: Vec<String> = each
        .iter()
        .map(|(name, hash)| format!("  {name}: {hash:016x}"))
        .collect();
    assert_eq!(
        line.as_deref(),
        Some(format!("{hash:016x}").as_str()),
        "engine {ENGINE} painted the fixed stroke set to {hash:016x}; per stroke:\n{}",
        each.join("\n")
    );
}

#[test]
fn a_stroke_of_another_engine_is_not_painted_by_this_one() {
    let case = &cases()[0];
    let mut kept = draw_live(
        case,
        &wave(24.0, 200.0, 40.0, 0.3, 0.0),
        3,
        &mut layer(false),
    );
    kept.engine = ENGINE + 1;
    let mut raster = layer(false);
    assert_eq!(
        replay::paint(
            &kept,
            &case.brush(),
            Resources::default(),
            &mut raster,
            [0, 0],
            false
        ),
        Err(ReplayError::Engine(ENGINE + 1))
    );
    assert_eq!(raster.bytes(), layer(false).bytes(), "nothing painted");
}

#[test]
fn the_recorder_keeps_the_stroke_in_layer_space_from_the_landing() {
    let mut settings = Settings::new(Device::Pen);
    settings.to_layer = [[0.5, 0.0, 10.0], [0.0, 0.5, -4.0]];
    let colors = Colors {
        main: [0.0; 3],
        sub: [1.0; 3],
    };
    let mut recorder = Recorder::new(ENGINE, [1; 32], &settings, colors, Vec::new());
    // Hovering before the landing is not part of the stroke.
    assert_eq!(
        recorder.take(Sample::new(Kind::Move, [0.0, 0.0], 99.0)),
        None
    );
    let down = recorder
        .take(Sample::new(Kind::Down, [20.0, 40.0], 100.0).pressing(0.4))
        .expect("the landing");
    assert_eq!(down.pos, [20.0, 16.0]);
    assert_eq!(down.time, 0.0);
    let moved = recorder
        .take(Sample::new(Kind::Move, [22.0, 40.0], 100.125))
        .expect("a move");
    assert_eq!(moved.time, 0.125);
    assert_eq!(moved.pressure, None, "an unsaid pressure stays unsaid");
    // The pen still, the device silent: the last sample again, later.
    let still = recorder.still(100.25).expect("held still");
    assert_eq!(
        (still.pos, still.time, still.kind),
        (moved.pos, 0.25, Kind::Move)
    );
    assert_eq!(recorder.still(100.25), None, "time goes forward");
    let up = recorder
        .take(Sample::new(Kind::Up, [22.0, 40.0], 100.3))
        .expect("the lift");
    assert_eq!(up.kind, Kind::Up);
    assert_eq!(
        recorder.take(Sample::new(Kind::Move, [1.0, 1.0], 101.0)),
        None
    );
    assert_eq!(recorder.still(102.0), None);
    let kept = recorder.finish();
    assert_eq!(kept.samples.len(), 4);
    assert_eq!(kept.settings().to_layer, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
}

#[test]
fn the_stored_stroke_keeps_what_it_does_not_know_and_refuses_what_it_cannot_read() {
    let mut settings = Settings::new(Device::Constant(0.7));
    settings.step = Some(0.5);
    settings.seed = 42;
    let colors = Colors {
        main: [0.2, 0.3, 0.4],
        sub: [0.5, 0.6, 0.7],
    };
    let mut stroke = StoredStroke::new(
        ENGINE,
        [9; 32],
        &settings,
        colors,
        vec![[1.0, 0.0, 0.0, -1.0, 0.0, 64.0]],
    );
    stroke.samples = wave(0.0, 10.0, 5.0, 0.05, 0.0);
    stroke
        .extra
        .push(("a newer field".to_owned(), vec![1, 2, 3]));
    let bytes = stroke.to_bytes();
    assert_eq!(StoredStroke::from_bytes(&bytes), Ok(stroke.clone()));
    assert_eq!(
        StoredStroke::from_bytes(&bytes).expect("reads").to_bytes(),
        bytes
    );
    // A newer format is refused, not misread.
    let mut newer = bytes.clone();
    newer[0] = 2;
    assert!(StoredStroke::from_bytes(&newer).is_err());
    // Cut short anywhere, it is refused.
    for end in 0..bytes.len() {
        assert!(
            StoredStroke::from_bytes(&bytes[..end]).is_err(),
            "cut at {end}"
        );
    }
}

#[test]
fn a_stroke_kept_by_format_1_still_reads() {
    // Written once by this build (the generator below), never again.
    let bytes = include_bytes!("golden/stroke-v1.bin");
    let stroke = StoredStroke::from_bytes(bytes).expect("a format 1 stroke reads");
    assert_eq!(stroke, fixture());
    assert_eq!(stroke.to_bytes(), bytes, "and writes back the same");
}

/// What `golden/stroke-v1.bin` holds.
fn fixture() -> StoredStroke {
    let mut settings = Settings::new(Device::Pen);
    settings.seed = 0x0123_4567_89AB_CDEF;
    settings.scale = 2.0;
    settings.curve = Response::new(&[[0.0, 0.1], [0.5, 0.4], [1.0, 1.0]]);
    let colors = Colors {
        main: [0.25, 0.5, 0.75],
        sub: [1.0, 0.5, 0.0],
    };
    let mut stroke = StoredStroke::new(
        1,
        [0xAB; 32],
        &settings,
        colors,
        vec![[-1.0, 0.0, 0.0, 1.0, 64.0, 0.0]],
    );
    stroke.samples = vec![
        Sample::new(Kind::Down, [1.0, 2.0], 0.0).pressing(0.5),
        Sample {
            tilt: Some([0.25, -0.5]),
            twist: Some(90.0),
            ..Sample::new(Kind::Move, [3.5, 2.0], 0.0078125)
        },
        Sample::new(Kind::Move, [6.0, 2.5], 0.015625).pressing(0.75),
        Sample::new(Kind::Up, [6.0, 2.5], 0.0234375),
    ];
    stroke
}

#[test]
#[ignore = "generator: the golden stroke is committed and never written again"]
fn generate_the_format_1_stroke() {
    let dir = std::env::temp_dir().join("sue-brush-golden");
    std::fs::create_dir_all(&dir).expect("a folder");
    std::fs::write(dir.join("stroke-v1.bin"), fixture().to_bytes()).expect("written");
}

#[test]
fn no_value_that_reaches_a_pixel_goes_through_the_system_math_library() {
    // The system's transcendental functions differ in their last bits
    // between platforms; the engine uses its own (`sue_brush::math`).
    // Only the code outside the tests is read.
    const SYSTEM: [&str; 22] = [
        ".sin(",
        ".cos(",
        ".tan(",
        ".sin_cos(",
        ".exp(",
        ".exp2(",
        ".exp_m1(",
        ".ln(",
        ".log(",
        ".log2(",
        ".log10(",
        ".ln_1p(",
        ".powf(",
        ".powi(",
        ".atan(",
        ".atan2(",
        ".asin(",
        ".acos(",
        ".sinh(",
        ".cosh(",
        ".hypot(",
        ".cbrt(",
    ];
    // Accessors of the engine's own types that share a name.
    const OURS: [&str; 2] = ["turn.cos()", "turn.sin()"];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![root];
    let mut found = Vec::new();
    let mut read = 0;
    while let Some(path) = files.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(&path).expect("a folder") {
                files.push(entry.expect("an entry").path());
            }
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        read += 1;
        let text = std::fs::read_to_string(&path).expect("a source file");
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for (n, line) in code.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            let mut rest = line.to_owned();
            for ours in OURS {
                rest = rest.replace(ours, "");
            }
            if SYSTEM.iter().any(|call| rest.contains(call)) {
                found.push(format!("{}:{}: {line}", path.display(), n + 1));
            }
        }
    }
    assert!(read > 30, "the engine's sources were read: {read}");
    assert!(
        found.is_empty(),
        "the system's math on the pixel path:\n{}",
        found.join("\n")
    );
}
