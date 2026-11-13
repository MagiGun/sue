//! Sue Paint: strokes, the pen, touch, and how fast frames are.

use super::*;

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
