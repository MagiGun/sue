//! Sue Paint: strokes, the pen, touch, and how fast frames are.

use super::*;
use sue_platform::stylus::{self, PenSource};

#[test]
fn sue_paint_smart_shape_straightens_a_held_stroke() {
    let (mut runtime, root) = runtime("paint-smart");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    // How tall the ink on the drawing layer is, in pixels.
    let ink_height = |runtime: &Runtime| {
        let arena = &runtime.engine.world.arena;
        let layer = sue_paint::doc::layers(arena, picture)
            .into_iter()
            .rfind(|info| info.kind == sue_model::picture::LayerKind::Raster)
            .unwrap()
            .id;
        let raster = sue_paint::doc::raster(arena, layer).unwrap();
        let width = raster.width() as usize;
        let rows: Vec<usize> = raster
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, pixel)| pixel[3] > 128)
            .map(|(index, _)| index / width)
            .collect();
        rows.iter()
            .max()
            .zip(rows.iter().min())
            .map_or(0, |(a, b)| a - b)
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: plain,
    };
    // A wavy stroke across the canvas with the pen, held still at its
    // end or not.
    step(&mut runtime, vec![key(egui::Key::P, plain)]);
    let middle = paint_middle(&runtime);
    let draw = |step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                runtime: &mut Runtime,
                hold: bool| {
        let from = middle - vec2(150.0, 0.0);
        step(
            runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        let mut at = from;
        for part in 1..=60 {
            let x = part as f32 * 5.0;
            at = from + vec2(x, (x / 20.0).sin() * 6.0);
            step(runtime, vec![egui::Event::PointerMoved(at)]);
        }
        if hold {
            for _ in 0..60 {
                step(runtime, Vec::new());
            }
        }
        step(runtime, vec![press(at, false)]);
        step(runtime, Vec::new());
    };
    draw(&mut step, &mut runtime, false);
    let wavy = ink_height(&runtime);
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(ink_height(&runtime), 0, "undone");
    let depth = runtime.engine.history.depth();
    draw(&mut step, &mut runtime, true);
    let straight = ink_height(&runtime);
    assert!(straight + 4 < wavy, "straightened: {straight} from {wavy}");
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    let _ = std::fs::remove_dir_all(&root);
}

/// Frame times of a huge brush on a big picture through the whole app
/// (`cargo test --release -p sue paint_frame_speed -- --ignored
/// --nocapture`).
#[test]
#[ignore = "a benchmark"]
fn paint_frame_speed() {
    let (mean, worst, lift, after) = paint_stroke_frames(&[], 40);
    println!(
        "4000x3000, 8 layers, brush huge: frame mean {mean:.2} ms, worst {worst:.2} ms, lift {lift:.2} ms, next {after:.2} ms"
    );
}

/// Frames of a stroke with Sue Paint's own brushes, each picked by its
/// shortcut (P for the pencil, B for the brushes, E for the eraser)
/// and made big (`cargo test --release -p sue paint_tools_frame_speed
/// -- --ignored --nocapture`).
#[test]
#[ignore = "a benchmark"]
fn paint_tools_frame_speed() {
    use egui::Key::{B, E, P};
    for (name, keys) in [
        ("G-pen", &[][..]),
        ("Pencil", &[P][..]),
        ("Watercolor", &[B][..]),
        ("Airbrush", &[B, B][..]),
        ("Eraser", &[E][..]),
    ] {
        for bigger in [15, 25, 32] {
            let (mean, worst, lift, _) = paint_stroke_frames(keys, bigger);
            println!(
                "{name:<12} {bigger} sizes up: frame mean {mean:>6.2} ms, worst {worst:>7.2} ms, lift {lift:>7.2} ms"
            );
        }
    }
}

/// A 4000 by 3000 picture of eight layers, `keys` pressed to pick a
/// tool, the brush `bigger` steps bigger, and a stroke across it: the
/// mean and worst frame, the lift and the frame after, in ms.
fn paint_stroke_frames(keys: &[egui::Key], bigger: usize) -> (f64, f64, f64, f64) {
    let (mut runtime, root) = runtime("paint-speed");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    let paint = AppletId::new(sue_paint::ID).unwrap();
    for (key, value) in [("sue_paint.width", 4000), ("sue_paint.height", 3000)] {
        let _ = runtime
            .engine
            .world
            .rice
            .set(key, sue_rice::RiceValue::Int(value));
    }
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        let begun = Instant::now();
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
        begun.elapsed().as_secs_f64() * 1000.0
    };
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    for _ in 0..6 {
        step(
            &mut runtime,
            vec![key(
                egui::Key::N,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            )],
        );
    }
    for &pick in keys {
        step(&mut runtime, vec![key(pick, egui::Modifiers::NONE)]);
    }
    // ] makes the brush bigger: from 8 pixels to the largest.
    for _ in 0..bigger {
        step(
            &mut runtime,
            vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)],
        );
    }
    for _ in 0..5 {
        step(&mut runtime, Vec::new());
    }
    let middle = paint_middle(&runtime);
    let from = middle - vec2(250.0, 0.0);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut times = Vec::new();
    times.push(step(
        &mut runtime,
        vec![egui::Event::PointerMoved(from), press(from, true)],
    ));
    let mut at = from;
    for _ in 0..120 {
        let a = at + vec2(2.0, 0.3);
        let b = a + vec2(2.0, -0.3);
        at = b;
        times.push(step(
            &mut runtime,
            vec![egui::Event::PointerMoved(a), egui::Event::PointerMoved(b)],
        ));
    }
    let lift = step(&mut runtime, vec![press(at, false)]);
    let after = step(&mut runtime, Vec::new());
    let mut slowest: Vec<(usize, f64)> = times.iter().copied().enumerate().collect();
    slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("slowest frames: {:?}", &slowest[..5]);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let worst = times.iter().copied().fold(0.0, f64::max);
    let _ = std::fs::remove_dir_all(&root);
    (mean, worst, lift, after)
}

/// After Smart Shape corrects a line, dragging with the pen still down
/// carries the line's far end with it, and lifting the pen keeps what
/// the drag left. Clip Studio Paint does the same, and a shape broken by
/// a stray stroke or a leftover of the old one would show here as ink
/// away from the line through the two ends.
#[test]
fn sue_paint_a_corrected_line_follows_the_pen_until_it_lifts() {
    let (mut runtime, root) = runtime("paint-smart-drag");
    let ctx = egui::Context::default();
    let mut now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    // The ink as it stands, as the points it covers.
    let ink = |runtime: &Runtime| -> Vec<(usize, usize)> {
        let arena = &runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        let raster = sue_paint::doc::raster(arena, layer).unwrap();
        let bytes = raster.bytes().unwrap();
        let width = raster.width() as usize;
        let height = raster.height() as usize;
        let mut points = Vec::new();
        for y in 0..height {
            for x in 0..width {
                if bytes[(y * width + x) * 4 + 3] > 128 {
                    points.push((x, y));
                }
            }
        }
        points
    };
    // How far the ink strays from the straight line it should be: the
    // widest distance off the line fitted through all of it.
    let straightness = |points: &[(usize, usize)]| -> f64 {
        let count = points.len() as f64;
        let mean_x = points.iter().map(|(x, _)| *x as f64).sum::<f64>() / count;
        let mean_y = points.iter().map(|(_, y)| *y as f64).sum::<f64>() / count;
        let (sxx, sxy, syy) = points.iter().fold((0.0, 0.0, 0.0), |acc, (x, y)| {
            let (dx, dy) = (*x as f64 - mean_x, *y as f64 - mean_y);
            (acc.0 + dx * dx, acc.1 + dx * dy, acc.2 + dy * dy)
        });
        // The direction the ink runs, and the normal to it.
        let angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let (sin, cos) = angle.sin_cos();
        points
            .iter()
            .map(|(x, y)| ((*x as f64 - mean_x) * -sin + (*y as f64 - mean_y) * cos).abs())
            .fold(0.0, f64::max)
    };
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: Some(0.6),
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    // The New dialog closes over its own frames, before the canvas
    // behind it takes a stroke.
    for _ in 0..5 {
        step(&mut runtime, Vec::new());
    }
    // A line drawn by hand: mostly straight, a little wobble.
    let start = paint_middle(&runtime) - vec2(140.0, 60.0);
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Start, start),
            egui::Event::PointerMoved(start),
            button(start, true),
        ],
    );
    let mut at = start;
    for index in 0..40 {
        at += vec2(7.0, 3.0);
        if index % 7 == 0 {
            at += vec2(0.0, 1.5);
        }
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ],
        );
    }
    // The pen rests: Smart Shape makes it the line it was meant to be.
    for _ in 0..70 {
        step(&mut runtime, Vec::new());
    }
    // How wide the line is: its ink spread over the way it runs.
    let width_of = |points: &[(usize, usize)]| -> f64 {
        let first = points[0];
        let last = points[points.len() - 1];
        let length = (last.0 as f64 - first.0 as f64)
            .hypot(last.1 as f64 - first.1 as f64)
            .max(1.0);
        points.len() as f64 / length
    };
    let corrected = ink(&runtime);
    assert!(corrected.len() > 100, "the line was drawn");
    assert!(
        straightness(&corrected) < width_of(&corrected) * 0.5 + 2.0,
        "the corrected line is straight: strays {:.1} px over {:.1} px of width",
        straightness(&corrected),
        width_of(&corrected)
    );
    let before = (corrected[0], corrected[corrected.len() - 1]);
    // Dragging, the pen still down, carries the far end down the canvas.
    for _ in 0..40 {
        at += vec2(0.0, 6.0);
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ],
        );
    }
    let dragged = ink(&runtime);
    assert!(
        straightness(&dragged) < width_of(&dragged) * 0.5 + 2.0,
        "the dragged line is still straight: strays {:.1} px over {:.1} px of width",
        straightness(&dragged),
        width_of(&dragged)
    );
    let after = (dragged[0], dragged[dragged.len() - 1]);
    assert!(
        after.0.1.abs_diff(before.0.1) <= 4,
        "the line's near end stayed put: {:?} against {:?}",
        after.0,
        before.0
    );
    assert!(
        after.1.1 > before.1.1 + 200,
        "the far end followed the pen: {:?} against {:?}",
        after.1,
        before.1
    );
    assert!(
        dragged.len() > corrected.len() + 100,
        "the line grew: {} px against {}",
        dragged.len(),
        corrected.len()
    );
    // Lifting the pen keeps what the drag left.
    step(
        &mut runtime,
        vec![touch(egui::TouchPhase::End, at), button(at, false)],
    );
    for _ in 0..5 {
        step(&mut runtime, Vec::new());
    }
    let released = ink(&runtime);
    assert!(
        released.len().abs_diff(dragged.len()) < dragged.len() / 100 + 5,
        "the line stayed as the drag left it: {} px against {}",
        released.len(),
        dragged.len()
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_a_light_pen_stroke_ends_light() {
    let (mut runtime, root) = runtime("paint-light-end");
    let ctx = egui::Context::default();
    let mut now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    // A bigger G-pen, so a full-pressure blob would show.
    for _ in 0..8 {
        step(
            &mut runtime,
            vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)],
        );
    }
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: Some(0.15),
    };
    let start = paint_middle(&runtime) - vec2(120.0, 0.0);
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Start, start),
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let mut at = start;
    for _ in 0..30 {
        at += vec2(8.0, 0.0);
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ],
        );
    }
    // A real pen lifts a little away from its last move.
    let lift = at + vec2(3.0, 0.0);
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::End, lift),
            egui::Event::PointerButton {
                pos: lift,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    step(&mut runtime, Vec::new());
    let arena = &runtime.engine.world.arena;
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .unwrap();
    let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
    let raster = sue_paint::doc::raster(arena, layer).unwrap();
    let bytes = raster.bytes().unwrap();
    let width = raster.width() as usize;
    // The painted columns, and each one's thickness.
    let thick = |x: usize| {
        (0..raster.height() as usize)
            .filter(|y| bytes[(y * width + x) * 4 + 3] > 128)
            .count()
    };
    let columns: Vec<usize> = (0..width).filter(|x| thick(*x) > 0).collect();
    assert!(columns.len() > 20, "the stroke was painted");
    let middle = thick(columns[columns.len() / 2]);
    let end = columns[columns.len().saturating_sub(12)..]
        .iter()
        .map(|x| thick(*x))
        .max()
        .unwrap();
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px)"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_pinching_takes_back_the_touch_and_undoes_nothing() {
    let (mut runtime, root) = runtime("paint-pinch");
    let ctx = egui::Context::default();
    let mut now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    let pictures = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::picture::is_picture(entity))
            .count()
    };
    let touch = |id: u64, phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    // A real stroke with one finger first.
    let a = middle - vec2(150.0, 80.0);
    step(
        &mut runtime,
        vec![
            touch(1, egui::TouchPhase::Start, a),
            egui::Event::PointerMoved(a),
            press(a, true),
        ],
    );
    step(
        &mut runtime,
        vec![
            touch(1, egui::TouchPhase::Move, a + vec2(60.0, 0.0)),
            egui::Event::PointerMoved(a + vec2(60.0, 0.0)),
        ],
    );
    step(
        &mut runtime,
        vec![
            touch(1, egui::TouchPhase::End, a + vec2(60.0, 0.0)),
            press(a + vec2(60.0, 0.0), false),
        ],
    );
    let depth = runtime.engine.history.depth();
    let open = pictures(&runtime);
    let layer_bytes = |runtime: &Runtime| {
        let arena = &runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        sue_paint::doc::raster(arena, layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec()
    };
    let before = layer_bytes(&runtime);
    // Now a pinch: one finger lands (and starts to paint), a second
    // follows, both spread apart and lift.
    let (one, two) = (middle, middle + vec2(80.0, 40.0));
    step(
        &mut runtime,
        vec![
            touch(1, egui::TouchPhase::Start, one),
            egui::Event::PointerMoved(one),
            press(one, true),
        ],
    );
    step(&mut runtime, vec![touch(2, egui::TouchPhase::Start, two)]);
    for spread in 1..=6 {
        let d = vec2(12.0, 6.0) * spread as f32;
        step(
            &mut runtime,
            vec![
                touch(1, egui::TouchPhase::Move, one - d),
                touch(2, egui::TouchPhase::Move, two + d),
                egui::Event::PointerMoved(one - d),
            ],
        );
    }
    step(
        &mut runtime,
        vec![
            touch(2, egui::TouchPhase::End, two + vec2(72.0, 36.0)),
            touch(1, egui::TouchPhase::End, one - vec2(72.0, 36.0)),
            press(one - vec2(72.0, 36.0), false),
        ],
    );
    step(&mut runtime, Vec::new());
    assert_eq!(
        runtime.engine.history.depth(),
        depth,
        "nothing undone, nothing added"
    );
    assert_eq!(pictures(&runtime), open, "the picture stays open");
    assert_eq!(
        layer_bytes(&runtime),
        before,
        "the pinch left no paint, the stroke stays"
    );
    // Both fingers landing in one frame: no stroke starts either.
    step(
        &mut runtime,
        vec![
            touch(3, egui::TouchPhase::Start, one),
            touch(4, egui::TouchPhase::Start, two),
            egui::Event::PointerMoved(one),
            press(one, true),
        ],
    );
    step(
        &mut runtime,
        vec![
            touch(3, egui::TouchPhase::Move, one - vec2(50.0, 0.0)),
            touch(4, egui::TouchPhase::Move, two + vec2(50.0, 0.0)),
            egui::Event::PointerMoved(one - vec2(50.0, 0.0)),
        ],
    );
    step(
        &mut runtime,
        vec![
            touch(3, egui::TouchPhase::End, one - vec2(50.0, 0.0)),
            touch(4, egui::TouchPhase::End, two + vec2(50.0, 0.0)),
            press(one - vec2(50.0, 0.0), false),
        ],
    );
    step(&mut runtime, Vec::new());
    assert_eq!(runtime.engine.history.depth(), depth);
    assert_eq!(layer_bytes(&runtime), before, "no stroke during a pinch");
    let _ = std::fs::remove_dir_all(&root);
}

// How gestures on the canvas end. A pen whose pressure comes from the
// tablet's driver and not with the window's touch events (WinTab, XInput2,
// a browser) must not end its strokes wider than it drew them, and every
// gesture ends where the button was let go.

/// Sue Paint open on a new canvas in a 1440 by 900 window.
struct Paint {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
}

impl Paint {
    fn new(tag: &str) -> Self {
        let (mut runtime, root) = runtime(tag);
        let ctx = egui::Context::default();
        let now = Instant::now();
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut paint = Self {
            runtime,
            ctx,
            now,
            root,
        };
        paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        paint.settle(10);
        paint
    }

    /// One frame with `events`, 16 thousandths of a second on.
    fn step(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let (runtime, now) = (&mut self.runtime, self.now);
        self.ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
            .drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// Runs one of Sue Paint's commands, as its menus do.
    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: AppletId::new(sue_paint::ID).unwrap(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
        self.step(Vec::new());
    }

    /// Where the canvas has its middle: the page's middle, moved by Sue
    /// Paint's palettes.
    fn middle(&self) -> Pos2 {
        self.runtime.shell.page_rect().center() + vec2(8.0, -63.0)
    }

    /// Presses the main button at `from` with `modifiers` held, moves to
    /// `to` in ten frames, and lets go at `lift` in a frame of its own
    /// with no move before it.
    fn drag(&mut self, from: Pos2, to: Pos2, lift: Pos2, modifiers: egui::Modifiers) {
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![button(from, true, modifiers)]);
        for index in 1..=10 {
            let at = from + (to - from) * (index as f32 / 10.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![button(lift, false, modifiers)]);
        self.settle(3);
    }

    /// Every layer of the picture, bottom first: where it lies and its
    /// pixels' width and bytes.
    fn layers(&self) -> Vec<([i64; 2], usize, Vec<u8>)> {
        let arena = &self.runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        sue_paint::doc::layers(arena, picture)
            .into_iter()
            .filter_map(|layer| {
                let raster = sue_paint::doc::raster(arena, layer.id)?;
                Some((
                    layer.style.offset,
                    raster.width() as usize,
                    raster.bytes()?.to_vec(),
                ))
            })
            .collect()
    }

    /// The top layer's ink as a line's profile: each painted column and
    /// how many of its pixels are more than half covered, left to right.
    fn profile(&self) -> Vec<(usize, usize)> {
        let layers = self.layers();
        let (_, width, bytes) = layers.last().unwrap();
        let height = bytes.len() / 4 / width;
        (0..*width)
            .map(|x| {
                let thick = (0..height)
                    .filter(|y| bytes[(y * width + x) * 4 + 3] > 128)
                    .count();
                (x, thick)
            })
            .filter(|(_, thick)| *thick > 0)
            .collect()
    }
}

impl Drop for Paint {
    fn drop(&mut self) {
        stylus::forget_for_test();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// How the tablet's driver and the window's pointer events fall in the
/// frame in which the pen lifts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lift {
    /// The driver says the pen has lifted, and the pen's last move and its
    /// button's release come in that same frame.
    SameFrame,
    /// The driver said so a frame before the button's release arrives, so
    /// by then no pressure at all is to be had.
    FrameLate,
    /// No pen: a mouse, which has no pressure from first to last.
    Mouse,
}

/// A line 240 points long with a bigger G-pen, pressed as hard as
/// `pressure` says and lifted as `lift` says, 3 points past its last move
/// as a real pen does: the line's profile.
fn stroke(tag: &str, pressure: f32, lift: Lift) -> Vec<(usize, usize)> {
    let mut paint = Paint::new(tag);
    // A bigger G-pen, so a full-pressure blob would show.
    for _ in 0..8 {
        paint.step(vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)]);
    }
    let pen = lift != Lift::Mouse;
    if pen {
        stylus::set_for_test(PenSource::WinTab, Some(pressure));
    }
    let start = paint.middle() - vec2(120.0, 0.0);
    paint.step(vec![
        egui::Event::PointerMoved(start),
        button(start, true, egui::Modifiers::NONE),
    ]);
    let mut at = start;
    for _ in 0..30 {
        at += vec2(8.0, 0.0);
        paint.step(vec![egui::Event::PointerMoved(at)]);
    }
    // The tip leaves the tablet: the driver's last packet of this frame
    // says so before the window's events of the frame are handled.
    if pen {
        stylus::set_for_test(PenSource::WinTab, None);
    }
    if lift == Lift::FrameLate {
        paint.step(Vec::new());
        stylus::forget_for_test();
    }
    let end = at + vec2(3.0, 0.0);
    paint.step(vec![
        egui::Event::PointerMoved(end),
        button(end, false, egui::Modifiers::NONE),
    ]);
    paint.settle(2);
    paint.profile()
}

/// The thickness of the line's middle and the most of its last 12 columns.
fn middle_and_end(profile: &[(usize, usize)]) -> (usize, usize) {
    assert!(profile.len() > 20, "the stroke was painted: {profile:?}");
    let middle = profile[profile.len() / 2].1;
    let end = profile[profile.len() - 12..]
        .iter()
        .map(|(_, thick)| *thick)
        .max()
        .unwrap();
    (middle, end)
}

/// The WinTab route of `sue_paint_a_light_pen_stroke_ends_light`: no touch
/// events at all, the pressure comes from the tablet's driver.
#[test]
fn sue_paint_a_light_driver_pen_stroke_ends_light() {
    let profile = stroke("paint-light-end-driver", 0.15, Lift::SameFrame);
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px): {:?}",
        &profile[profile.len() - 16..]
    );
}

/// The same when the button's release reaches the window a frame after the
/// driver said the pen lifted: nothing has the pen's pressure any more, and
/// the stroke ends with its own last pressure, not a full one.
#[test]
fn sue_paint_a_light_driver_pen_stroke_ends_light_a_frame_late() {
    let profile = stroke("paint-light-end-late", 0.15, Lift::FrameLate);
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px): {:?}",
        &profile[profile.len() - 16..]
    );
}

/// A mouse draws at full size from first to last, and ends no wider.
#[test]
fn sue_paint_a_mouse_stroke_ends_no_wider_than_it_is() {
    let profile = stroke("paint-mouse-end", 1.0, Lift::Mouse);
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px): {:?}",
        &profile[profile.len() - 16..]
    );
    let light = stroke("paint-mouse-end-light", 0.15, Lift::SameFrame);
    assert!(
        middle > middle_and_end(&light).0,
        "a mouse draws at the brush's full size"
    );
}

/// What a drag leaves when the button is let go somewhere the pointer was
/// never reported to have moved to: with `before` run first, the tool
/// picked by `tool`, the drag from `from` to `to` let go at `lift`, and
/// `after` run last.
fn gesture(
    tag: &str,
    before: bool,
    tool: &[&str],
    modifiers: egui::Modifiers,
    [to, lift]: [egui::Vec2; 2],
    after: &[&str],
) -> Vec<([i64; 2], usize, Vec<u8>)> {
    let mut paint = Paint::new(tag);
    let from = paint.middle() - vec2(100.0, 40.0);
    if before {
        // Something to move: a short line of the pen.
        paint.drag(
            from,
            from + vec2(60.0, 0.0),
            from + vec2(60.0, 0.0),
            egui::Modifiers::NONE,
        );
    }
    for command in tool {
        paint.command(command);
    }
    paint.drag(from, from + to, from + lift, modifiers);
    for command in after {
        paint.command(command);
    }
    paint.settle(3);
    paint.layers()
}

/// A gesture to try: its name, whether something is drawn first, the
/// commands that pick its tool, the keys held, and the commands run after.
type Case = (
    &'static str,
    bool,
    &'static [&'static str],
    egui::Modifiers,
    &'static [&'static str],
);

/// A release event is the gesture's last position even when no move to it
/// came first: a rectangle selection, a straight line (the Figure tool's,
/// and a pen's with Shift), a gradient and Move Layer end where the button
/// was let go.
#[test]
fn sue_paint_gestures_end_where_the_button_is_let_go() {
    let (near, far) = (vec2(120.0, 60.0), vec2(150.0, 84.0));
    let plain = egui::Modifiers::NONE;
    let cases: [Case; 5] = [
        ("select", false, &["tool.selection"], plain, &["edit.fill"]),
        ("figure-line", false, &["tool.figure"], plain, &[]),
        (
            "shift-line",
            false,
            &["tool.pen"],
            egui::Modifiers::SHIFT,
            &[],
        ),
        ("gradient", false, &["tool.fill", "tool.fill"], plain, &[]),
        ("move-layer", true, &["tool.move_layer"], plain, &[]),
    ];
    for (name, before, tool, modifiers, after) in cases {
        let run = |tag: &str, ends: [egui::Vec2; 2]| {
            gesture(
                &format!("paint-end-{name}-{tag}"),
                before,
                tool,
                modifiers,
                ends,
                after,
            )
        };
        let short = run("short", [near, near]);
        let long = run("long", [far, far]);
        let let_go = run("let-go", [near, far]);
        assert!(
            short != long,
            "{name}: the gesture's end shows in the picture"
        );
        assert!(
            let_go == long,
            "{name}: let go past the last move, it ends where it was let go"
        );
    }
}

/// A brush stroke too: its line reaches where the button was let go.
#[test]
fn sue_paint_a_stroke_reaches_where_the_button_is_let_go() {
    let last = |tag: &str, lift: f32| {
        let mut paint = Paint::new(tag);
        let from = paint.middle() - vec2(100.0, 0.0);
        let to = from + vec2(120.0, 0.0);
        paint.drag(from, to, to + vec2(lift, 0.0), egui::Modifiers::NONE);
        paint.profile().last().map_or(0, |(x, _)| *x)
    };
    let (at_move, past) = (
        last("paint-stroke-lift-0", 0.0),
        last("paint-stroke-lift-40", 40.0),
    );
    assert!(at_move > 0, "the stroke was painted");
    assert!(
        past >= at_move + 10,
        "let go 40 points on, the line ends at {past}, not at {at_move}"
    );
}

/// One frame of a sheet stroke: how far the pen moves in it (points) and
/// how hard it presses.
type Leg = (egui::Vec2, f32);

/// The strokes of the sheet, each as its frames: a light flick, a slow
/// lift, a fast lift and a hooked end.
fn sheet_strokes() -> [(&'static str, Vec<Leg>); 4] {
    let flick = (0..12)
        .map(|frame| (vec2(6.0 + frame as f32 * 2.2, -0.4 * frame as f32), 0.25))
        .collect();
    let slow = (0..30)
        .map(|frame| {
            let left = (30 - frame) as f32 / 30.0;
            (vec2(1.0 + 9.0 * left, 0.0), 0.15 + 0.4 * left)
        })
        .collect();
    let fast = (0..14).map(|_| (vec2(13.0, 0.0), 0.4)).collect();
    let mut hooked: Vec<Leg> = (0..22).map(|_| (vec2(8.0, 0.0), 0.4)).collect();
    hooked.extend([
        (vec2(1.0, 3.0), 0.3),
        (vec2(-2.0, 3.0), 0.2),
        (vec2(-2.0, 2.0), 0.1),
    ]);
    [
        ("light flick", flick),
        ("slow lift", slow),
        ("fast lift", fast),
        ("hooked end", hooked),
    ]
}

/// Writes a picture of stroke ends for a person to look at: four strokes
/// (rows: a light flick, a slow lift, a fast lift, a hooked end) by three
/// routes (columns: a driver's pen lifting in the release's frame, the
/// same with the release a frame late, a mouse), drawn through the whole
/// runtime with a bigger G-pen. `SUE_STROKE_ENDS` names the PNG file.
#[test]
#[ignore = "writes a picture of stroke ends for a person to look at"]
fn stroke_ends_sheet() {
    let mut paint = Paint::new("paint-stroke-ends-sheet");
    for _ in 0..8 {
        paint.step(vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)]);
    }
    let middle = paint.middle();
    let routes = [Lift::SameFrame, Lift::FrameLate, Lift::Mouse];
    for (row, (_, legs)) in sheet_strokes().into_iter().enumerate() {
        for (column, route) in routes.into_iter().enumerate() {
            let pen = route != Lift::Mouse;
            let start = middle + vec2(-330.0 + column as f32 * 195.0, -120.0 + row as f32 * 70.0);
            let first = legs.first().map_or(1.0, |leg| leg.1);
            if pen {
                stylus::set_for_test(PenSource::WinTab, Some(first));
            }
            paint.step(vec![egui::Event::PointerMoved(start)]);
            paint.step(vec![button(start, true, egui::Modifiers::NONE)]);
            let mut at = start;
            let mut step = vec2(0.0, 0.0);
            for (by, pressure) in &legs {
                at += *by;
                step = *by;
                if pen {
                    stylus::set_for_test(PenSource::WinTab, Some(*pressure));
                }
                paint.step(vec![egui::Event::PointerMoved(at)]);
            }
            if pen {
                stylus::set_for_test(PenSource::WinTab, None);
            }
            if route == Lift::FrameLate {
                paint.step(Vec::new());
                stylus::forget_for_test();
            }
            // The pen leaves the tablet a third of a frame's way on.
            let end = at + step * 0.35;
            paint.step(vec![
                egui::Event::PointerMoved(end),
                button(end, false, egui::Modifiers::NONE),
            ]);
            stylus::forget_for_test();
            paint.settle(3);
        }
    }
    // The ink over white, cut to where it is and three times as large.
    let layers = paint.layers();
    let (_, width, bytes) = layers.last().unwrap();
    let height = bytes.len() / 4 / width;
    let alpha = |x: usize, y: usize| bytes[(y * width + x) * 4 + 3];
    let inked: Vec<(usize, usize)> = (0..height)
        .flat_map(|y| (0..*width).map(move |x| (x, y)))
        .filter(|(x, y)| alpha(*x, *y) > 0)
        .collect();
    assert!(!inked.is_empty(), "the strokes were painted");
    let edge = |pick: fn(&(usize, usize)) -> usize| {
        let values = inked.iter().map(pick);
        (
            values.clone().min().unwrap_or(0).saturating_sub(12),
            values.max().unwrap_or(0) + 12,
        )
    };
    let ((x0, x1), (y0, y1)) = (edge(|at| at.0), edge(|at| at.1));
    let (x1, y1) = (x1.min(width - 1), y1.min(height - 1));
    let scale = 3;
    let (out_w, out_h) = ((x1 - x0 + 1) * scale, (y1 - y0 + 1) * scale);
    let mut out = Vec::with_capacity(out_w * out_h * 4);
    for y in 0..out_h {
        for x in 0..out_w {
            let at = ((y0 + y / scale) * width + x0 + x / scale) * 4;
            let cover = u32::from(bytes[at + 3]);
            for channel in 0..3 {
                let ink = u32::from(bytes[at + channel]);
                out.push(((ink * cover + 255 * (255 - cover)) / 255) as u8);
            }
            out.push(255);
        }
    }
    let png = sue_io::encode_png_rgba(out_w as u32, out_h as u32, &out).unwrap();
    let path = std::env::var("SUE_STROKE_ENDS").map_or_else(
        |_| std::env::temp_dir().join("sue-stroke-ends.png"),
        PathBuf::from,
    );
    std::fs::write(&path, png).unwrap();
    println!("stroke ends: {}", path.display());
}

/// Sue Paint with the new brush engine on (`sue_paint.brush_engine`), and
/// a bigger G-pen, so a line's width can be read.
fn new_engine(tag: &str) -> Paint {
    let mut paint = Paint::new(tag);
    // The key is hidden (not in the shipped rice): as the owner turns it
    // on, by writing it into the rice file.
    let _ = paint
        .runtime
        .engine
        .world
        .rice
        .register(&sue_rice::RiceDeclaration::new(
            "sue_paint.brush_engine",
            sue_rice::RiceParameter::Text {
                value: "new".to_owned(),
            },
            "",
        ));
    for _ in 0..8 {
        paint.step(vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)]);
    }
    paint
}

/// A sample of the pen's stream at `pos` and `time`, pressing `pressure`.
fn packet(kind: stylus::PenKind, pos: Pos2, time: f64, pressure: Option<f32>) -> stylus::PenSample {
    let mut sample = stylus::PenSample::at(PenSource::WinTab, [pos.x, pos.y], time);
    sample.kind = kind;
    sample.pressure = pressure;
    sample
}

/// The new engine takes every packet of the pen with its own pressure:
/// forty packets that press harder and harder, all in one frame, draw a
/// line that widens along its length, not one of the frame's last
/// pressure. The lift is a frame later and adds nothing; the stroke is
/// one undo step.
#[test]
fn sue_paint_the_new_engine_draws_every_packets_pressure() {
    let mut paint = new_engine("paint-engine-packets");
    let start = paint.middle() - vec2(150.0, 0.0);
    let mut events = vec![
        egui::Event::PointerMoved(start),
        button(start, true, egui::Modifiers::NONE),
    ];
    let count = 40;
    for i in 0..=count {
        let at = start + vec2(300.0 * i as f32 / count as f32, 0.0);
        let pressure = 0.1 + 0.9 * i as f32 / count as f32;
        let kind = if i == 0 {
            stylus::PenKind::Down
        } else {
            stylus::PenKind::Move
        };
        stylus::push_for_test(packet(kind, at, 5.0 + f64::from(i) / 200.0, Some(pressure)));
        if i > 0 && i % 4 == 0 {
            events.push(egui::Event::PointerMoved(at));
        }
    }
    paint.step(events);
    let end = start + vec2(300.0, 0.0);
    stylus::push_for_test(packet(stylus::PenKind::Up, end, 5.25, None));
    paint.step(vec![button(end, false, egui::Modifiers::NONE)]);
    paint.settle(3);
    let profile = paint.profile();
    assert!(profile.len() > 40, "the line was drawn: {profile:?}");
    let quarter = profile.len() / 4;
    let early = profile[quarter].1;
    let late = profile[profile.len() - quarter].1;
    assert!(
        early * 3 <= late * 2,
        "the line widens with the pen's pressure within one frame: {early} px early, {late} px late"
    );
    paint.command("edit.undo");
    assert!(paint.profile().is_empty(), "the stroke was one undo step");
    paint.command("edit.redo");
    assert_eq!(paint.profile(), profile, "redo lays it again");
}

/// With no sample of the pen's stream (a script of pointer events), the
/// pointer's events are the new engine's samples; its end is no blob.
#[test]
fn sue_paint_the_new_engine_draws_from_pointer_events_too() {
    let mut paint = new_engine("paint-engine-pointer");
    let start = paint.middle() - vec2(120.0, 0.0);
    paint.step(vec![
        egui::Event::PointerMoved(start),
        button(start, true, egui::Modifiers::NONE),
    ]);
    let mut at = start;
    for _ in 0..30 {
        at += vec2(8.0, 0.0);
        paint.step(vec![egui::Event::PointerMoved(at)]);
    }
    let end = at + vec2(3.0, 0.0);
    paint.step(vec![
        egui::Event::PointerMoved(end),
        button(end, false, egui::Modifiers::NONE),
    ]);
    paint.settle(2);
    let profile = paint.profile();
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px)"
    );
    paint.command("edit.undo");
    assert!(paint.profile().is_empty(), "the stroke was one undo step");
}
