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
/// away from the line through the two ends. The brush engine shows a
/// stroke over its layer until the lift, so the ink is read once the pen
/// lifted: rested and lifted, and rested, dragged and lifted.
#[test]
fn sue_paint_a_corrected_line_follows_the_pen_until_it_lifts() {
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
    // How wide the line is: its ink spread over the way it runs.
    let width_of = |points: &[(usize, usize)]| -> f64 {
        let first = points[0];
        let last = points[points.len() - 1];
        let length = (last.0 as f64 - first.0 as f64)
            .hypot(last.1 as f64 - first.1 as f64)
            .max(1.0);
        points.len() as f64 / length
    };
    // A line drawn by hand (mostly straight, a little wobble), rested on
    // until Smart Shape makes it the line it was meant to be, dragged
    // down `drag` times 6 points, and lifted: the ink it leaves.
    let line = |tag: &str, drag: usize| -> Vec<(usize, usize)> {
        let mut paint = Paint::new(tag);
        let touch = |phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: Some(0.6),
        };
        let start = paint.middle() - vec2(140.0, 60.0);
        paint.step(vec![
            touch(egui::TouchPhase::Start, start),
            egui::Event::PointerMoved(start),
            button(start, true, egui::Modifiers::NONE),
        ]);
        let mut at = start;
        for index in 0..40 {
            at += vec2(7.0, 3.0);
            if index % 7 == 0 {
                at += vec2(0.0, 1.5);
            }
            paint.step(vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ]);
        }
        paint.settle(70);
        for _ in 0..drag {
            at += vec2(0.0, 6.0);
            paint.step(vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ]);
        }
        paint.step(vec![
            touch(egui::TouchPhase::End, at),
            button(at, false, egui::Modifiers::NONE),
        ]);
        paint.settle(5);
        paint.ink()
    };
    let corrected = line("paint-smart-rest", 0);
    assert!(corrected.len() > 100, "the line was drawn");
    assert!(
        straightness(&corrected) < width_of(&corrected) * 0.5 + 2.0,
        "the corrected line is straight: strays {:.1} px over {:.1} px of width",
        straightness(&corrected),
        width_of(&corrected)
    );
    let before = (corrected[0], corrected[corrected.len() - 1]);
    let dragged = line("paint-smart-drag", 40);
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

    /// The top layer's ink as the pixels more than half covered, row by
    /// row.
    fn ink(&self) -> Vec<(usize, usize)> {
        let layers = self.layers();
        let (_, width, bytes) = layers.last().unwrap();
        let height = bytes.len() / 4 / width;
        (0..height)
            .flat_map(|y| (0..*width).map(move |x| (x, y)))
            .filter(|(x, y)| bytes[(y * width + x) * 4 + 3] > 128)
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

/// Sue Paint with a bigger G-pen, so a line's width can be read. The
/// brush engine draws it: it is what Sue Paint paints with.
fn new_engine(tag: &str) -> Paint {
    let mut paint = Paint::new(tag);
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

/// What the new engine lays on the real canvas from a pen's packets, for
/// a person to look at: rows of strokes, each frame carrying several
/// packets, ending four ways (columns: easing off, lifted while pressing,
/// the driver going quiet before the lift, a quick flick of falling
/// pressure). `SUE_STROKE_ENDS` names the PNG file.
#[test]
#[ignore = "writes a picture of the new engine's strokes for a person to look at"]
fn new_engine_sheet() {
    let mut paint = new_engine("paint-engine-sheet");
    let middle = paint.middle();
    let mut time = 10.0;
    for row in 0..4 {
        for column in 0..4 {
            let start = middle + vec2(-340.0 + column as f32 * 175.0, -150.0 + row as f32 * 90.0);
            let count = 60;
            let mut events = vec![
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::NONE),
            ];
            for i in 0..=count {
                let s = i as f32 / count as f32;
                let at = start + vec2(150.0 * s, 26.0 * (s * 5.0 + row as f32).sin());
                let body = 0.2 + 0.25 * row as f32;
                let rise = (s * 8.0).min(1.0);
                let pressure = match column {
                    0 => body * rise * (1.0 - ((s - 0.8).max(0.0) / 0.2) * 0.9),
                    1 => body * rise,
                    2 if s > 0.9 => -1.0,
                    3 => body * rise * (1.0 - s * 0.95),
                    _ => body * rise,
                };
                let kind = if i == 0 {
                    stylus::PenKind::Down
                } else {
                    stylus::PenKind::Move
                };
                time += 1.0 / 200.0;
                let pressure = (pressure >= 0.0).then_some(pressure.max(0.02));
                stylus::push_for_test(packet(kind, at, time, pressure));
                if i % 3 == 0 {
                    events.push(egui::Event::PointerMoved(at));
                }
                // About three packets reach a frame.
                if i % 3 == 2 {
                    paint.step(std::mem::take(&mut events));
                }
            }
            let end = start + vec2(150.0, 26.0 * (5.0 + row as f32).sin());
            paint.step(std::mem::take(&mut events));
            time += 1.0 / 200.0;
            stylus::push_for_test(packet(stylus::PenKind::Up, end, time, None));
            paint.step(vec![button(end, false, egui::Modifiers::NONE)]);
            paint.settle(2);
        }
    }
    let layers = paint.layers();
    let (_, width, bytes) = layers.last().unwrap();
    let height = bytes.len() / 4 / width;
    let mut out = Vec::with_capacity(width * height * 4);
    for pixel in bytes.chunks(4) {
        let cover = u32::from(pixel[3]);
        for &ink in &pixel[..3] {
            let ink = u32::from(ink);
            out.push(((ink * cover + 255 * (255 - cover)) / 255) as u8);
        }
        out.push(255);
    }
    let png = sue_io::encode_png_rgba(*width as u32, height as u32, &out).unwrap();
    let path = std::env::var("SUE_STROKE_ENDS").map_or_else(
        |_| std::env::temp_dir().join("sue-new-engine.png"),
        PathBuf::from,
    );
    std::fs::write(&path, png).unwrap();
    println!("new engine: {}", path.display());
}

// The brush engine is what Sue Paint paints with (P4-1a): the strokes
// the compositor cannot show live (a mask, a selection, locked
// transparent pixels) are laid straight into their rasters, Shift draws
// a straight line, a layer with keyframes on takes no stroke, and old
// masks become gray when their picture opens.

impl Paint {
    /// The top layer's id.
    fn top(&self) -> sue_core::EntityId {
        let arena = &self.runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        sue_paint::doc::layers(arena, picture).last().unwrap().id
    }

    /// The top layer's mask: its pixel format and bytes.
    fn mask(&self) -> Option<(sue_core::PixelFormat, Vec<u8>)> {
        let arena = &self.runtime.engine.world.arena;
        let mask = sue_paint::doc::mask_of(arena, self.top())?;
        let raster = sue_paint::doc::raster(arena, mask)?;
        Some((raster.format(), raster.bytes()?.to_vec()))
    }

    /// A slow stroke from `from` to `to`, 6 points a frame, lifted where
    /// it last moved: no flick.
    fn slow(&mut self, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![button(from, true, modifiers)]);
        let frames = (from.distance(to) / 6.0).ceil().max(1.0) as usize;
        for index in 1..=frames {
            let at = from + (to - from) * (index as f32 / frames as f32);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![button(to, false, modifiers)]);
        self.settle(3);
    }

    /// The text of Sue's notice, if one shows.
    fn notice(&self) -> String {
        self.runtime
            .shell
            .message()
            .map(|(text, _)| text.to_owned())
            .unwrap_or_default()
    }
}

/// The Eraser on a layer's mask hides the layer where it goes, in the
/// mask's own gray pixels, and leaves the layer's pixels alone; the Pen
/// shows it again; each stroke is one undo step.
#[test]
fn sue_paint_strokes_on_a_mask_paint_its_gray_pixels() {
    let mut paint = new_engine("paint-engine-mask");
    paint.command("select.all");
    paint.command("edit.fill");
    paint.command("layer.mask.outside");
    paint.command("select.none");
    let layer = paint.layers().last().unwrap().2.clone();
    let (format, shown) = paint.mask().expect("a mask");
    assert_eq!(format, sue_core::PixelFormat::Gray8, "a new mask is gray");
    assert!(shown.iter().all(|&v| v == 255), "it shows the whole layer");
    paint.command("layer.mask.edit");
    paint.command("tool.eraser");
    let from = paint.middle() - vec2(120.0, 0.0);
    let to = paint.middle() + vec2(120.0, 0.0);
    paint.slow(from, to, egui::Modifiers::NONE);
    let (format, hidden) = paint.mask().unwrap();
    assert_eq!(format, sue_core::PixelFormat::Gray8);
    let gone = hidden.iter().filter(|&&v| v < 128).count();
    assert!(gone > 500, "the eraser hid a line of it: {gone} px");
    assert_eq!(
        paint.layers().last().unwrap().2,
        layer,
        "the layer's own pixels are untouched"
    );
    paint.command("tool.pen");
    paint.slow(from, to, egui::Modifiers::NONE);
    let (_, again) = paint.mask().unwrap();
    let back = again.iter().filter(|&&v| v < 128).count();
    assert!(back < gone / 4, "the pen shows it again: {back} px hidden");
    paint.command("edit.undo");
    assert_eq!(paint.mask().unwrap().1, hidden, "undo takes the pen back");
    paint.command("edit.undo");
    assert_eq!(paint.mask().unwrap().1, shown, "and the eraser");
    paint.command("edit.redo");
    assert_eq!(paint.mask().unwrap().1, hidden, "redo lays it again");
}

/// A mask that sits apart from its layer (its own place, or unlinked) is
/// painted where it sits on the canvas, not where the layer does.
#[test]
fn sue_paint_a_mask_apart_from_its_layer_is_painted_where_it_sits() {
    let mut paint = new_engine("paint-engine-mask-apart");
    let from = paint.middle() - vec2(120.0, 0.0);
    let to = paint.middle() + vec2(120.0, 0.0);
    paint.command("select.all");
    paint.command("layer.mask.outside");
    paint.command("select.none");
    paint.command("layer.mask.edit");
    paint.command("tool.eraser");
    // The first column of the mask an eraser stroke hides.
    let first = |paint: &mut Paint| {
        paint.slow(from, to, egui::Modifiers::NONE);
        let (_, bytes) = paint.mask().unwrap();
        let width = paint.layers().last().unwrap().1;
        let first = (0..width)
            .find(|&x| bytes.iter().skip(x).step_by(width).any(|&v| v < 128))
            .expect("the eraser hid some of it");
        paint.command("edit.undo");
        first
    };
    let at_layer = first(&mut paint);
    // The mask moved 40 pixels right of its layer.
    let mask = {
        let arena = &paint.runtime.engine.world.arena;
        sue_paint::doc::mask_of(arena, paint.top()).unwrap()
    };
    let mut edit = sue_paint::doc::Edit::new("Move").then(sue_core::txn::SetProperty::new(
        mask,
        sue_model::picture::X,
        40_i64,
    ));
    sue_core::ReversibleTransaction::apply(&mut edit, &mut paint.runtime.engine.world.arena)
        .unwrap();
    let apart = first(&mut paint);
    assert!(
        apart.abs_diff(at_layer - 40) <= 1,
        "the stroke landed where the mask sits: column {apart}, with the layer {at_layer}"
    );
}

/// A stroke stays inside the selection: it is cut where the selection
/// ends, and its undo takes it all back.
#[test]
fn sue_paint_a_stroke_stays_inside_the_selection() {
    let mut paint = new_engine("paint-engine-selection");
    let middle = paint.middle();
    let (from, to) = (middle - vec2(120.0, 0.0), middle + vec2(120.0, 0.0));
    paint.slow(from, to, egui::Modifiers::NONE);
    let whole = paint.profile();
    assert!(whole.len() > 100, "the stroke was drawn");
    paint.command("edit.undo");
    // The selection: the stroke's left half.
    paint.command("tool.selection");
    paint.drag(
        middle - vec2(200.0, 80.0),
        middle + vec2(0.0, 80.0),
        middle + vec2(0.0, 80.0),
        egui::Modifiers::NONE,
    );
    paint.command("tool.pen");
    paint.slow(from, to, egui::Modifiers::NONE);
    let cut = paint.profile();
    assert!(!cut.is_empty(), "the selected part was drawn");
    let (first, last) = (cut[0].0, cut[cut.len() - 1].0);
    assert!(first.abs_diff(whole[0].0) <= 2, "it starts where it did");
    let middle_x = (whole[0].0 + whole[whole.len() - 1].0) / 2;
    assert!(
        last.abs_diff(middle_x) <= 4,
        "it ends where the selection does: {last}, the middle {middle_x}"
    );
    paint.command("edit.undo");
    assert!(paint.profile().is_empty(), "one undo step takes it back");
}

/// With the layer's transparent pixels locked, a stroke paints only where
/// the layer has paint already: crossing a line adds no pixel.
#[test]
fn sue_paint_locked_transparent_pixels_stay_transparent() {
    let mut paint = new_engine("paint-engine-lock");
    let middle = paint.middle();
    paint.slow(
        middle - vec2(120.0, 0.0),
        middle + vec2(120.0, 0.0),
        egui::Modifiers::NONE,
    );
    let inked = paint.ink();
    paint.command("layer.toggle.lock_alpha");
    paint.slow(
        middle - vec2(0.0, 100.0),
        middle + vec2(0.0, 100.0),
        egui::Modifiers::NONE,
    );
    assert_eq!(paint.ink(), inked, "no pixel was added off the line");
}

/// Shift and a drag draw one straight line from the press to the lift,
/// whatever way the pen went between.
#[test]
fn sue_paint_shift_draws_a_straight_line() {
    let mut paint = new_engine("paint-engine-shift");
    let from = paint.middle() - vec2(150.0, 0.0);
    let shift = egui::Modifiers::SHIFT;
    paint.step(vec![egui::Event::PointerMoved(from)]);
    paint.step(vec![button(from, true, shift)]);
    let legs = [
        vec2(0.0, 0.0),
        vec2(60.0, 60.0),
        vec2(120.0, -70.0),
        vec2(300.0, 0.0),
    ];
    for leg in legs.windows(2) {
        for step in 1..=10 {
            let at = from + leg[0] + (leg[1] - leg[0]) * (step as f32 / 10.0);
            paint.step(vec![egui::Event::PointerMoved(at)]);
        }
    }
    let to = from + vec2(300.0, 0.0);
    paint.step(vec![button(to, false, shift)]);
    paint.settle(3);
    let ink = paint.ink();
    assert!(!ink.is_empty(), "the line was drawn");
    let (top, bottom) = (
        ink.iter().map(|(_, y)| *y).min().unwrap(),
        ink.iter().map(|(_, y)| *y).max().unwrap(),
    );
    let thick = paint
        .profile()
        .iter()
        .map(|(_, thick)| *thick)
        .max()
        .unwrap();
    assert!(
        bottom - top <= thick + 4,
        "one straight line, not the way the pen went: rows {top} to {bottom}, {thick} px thick"
    );
    assert!(paint.profile().len() > 150, "from the press to the lift");
}

/// A layer whose keyframes are on takes no stroke: nothing is painted,
/// nothing goes on the undo stack, and a notice says why.
#[test]
fn sue_paint_a_layer_with_keyframes_on_refuses_strokes() {
    let mut paint = new_engine("paint-engine-keyframes");
    let layer = paint.top();
    let mut on = sue_paint::anim::motion::enable_keyframes(layer, true);
    sue_core::ReversibleTransaction::apply(&mut on, &mut paint.runtime.engine.world.arena).unwrap();
    let depth = paint.runtime.engine.history.depth();
    let middle = paint.middle();
    paint.slow(
        middle - vec2(100.0, 0.0),
        middle + vec2(100.0, 0.0),
        egui::Modifiers::NONE,
    );
    assert!(paint.profile().is_empty(), "nothing was painted");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth,
        "nothing to undo"
    );
    assert!(
        paint.notice().to_lowercase().contains("keyframes"),
        "a notice says why: {:?}",
        paint.notice()
    );
}

/// A picture with a mask as 0.0.75 wrote it (RGBA, its alpha the cover),
/// as a file brings it into the arena: once Sue Paint shows it, the mask
/// is gray with the same cover, the picture looks the same, nothing is on
/// the undo stack and the picture is not unsaved.
#[test]
fn sue_paint_old_rgba_masks_become_gray_and_the_picture_stays_clean() {
    let (mut runtime, root) = runtime("paint-engine-old-mask");
    let (mut made, picture) = sue_paint::doc::create(&sue_paint::doc::NewPicture {
        name: "Old".to_owned(),
        width: 64,
        height: 32,
        ..sue_paint::doc::NewPicture::default()
    })
    .unwrap();
    let arena = &mut runtime.engine.world.arena;
    sue_core::ReversibleTransaction::apply(&mut made, arena).unwrap();
    let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
    if let Some(bytes) =
        sue_paint::doc::raster_mut(arena, layer).and_then(sue_core::RasterTile2D::bytes_mut)
    {
        bytes.fill(255);
    }
    let covers: Vec<u8> = (0..64 * 32).map(|i| ((i % 64) * 4) as u8).collect();
    let pixels = covers.iter().flat_map(|&a| [255, 255, 255, a]).collect();
    let old = sue_core::Entity::new(
        "Mask",
        sue_core::RasterTile2D::from_bytes(64, 32, sue_core::PixelFormat::Rgba8, pixels).unwrap(),
    )
    .with_class(sue_paint::doc::MASK_CLASS);
    let mask = old.id();
    arena.insert(old);
    arena
        .relate(sue_core::Relation::new(layer, sue_paint::doc::MASK, mask))
        .unwrap();
    let looks = |arena: &sue_core::Arena| {
        sue_paint::composite::render(
            arena,
            picture,
            [0, 0, 64, 32],
            sue_paint::composite::RenderOptions::default(),
        )
    };
    let before = looks(&runtime.engine.world.arena);
    let depth = runtime.engine.history.depth();
    let ctx = egui::Context::default();
    let now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let arena = &runtime.engine.world.arena;
    let tile = sue_paint::doc::raster(arena, mask).unwrap();
    assert_eq!(
        tile.format(),
        sue_core::PixelFormat::Gray8,
        "the mask is gray"
    );
    assert_eq!(
        tile.bytes().unwrap(),
        covers.as_slice(),
        "with the same cover"
    );
    assert_eq!(
        sue_paint::doc::mask_of(arena, layer),
        Some(mask),
        "the same mask"
    );
    assert_eq!(looks(arena), before, "the picture looks as it did");
    assert_eq!(runtime.engine.history.depth(), depth, "nothing to undo");
    let unsaved = runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .map(|applet| applet.unsaved(&runtime.engine.world.arena))
        .unwrap_or_default();
    assert!(
        unsaved.is_empty(),
        "the picture is not unsaved: {unsaved:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// One stroke of the strokes sheet: its frames (how far the pen moves in
/// each, in points, and how hard it presses) and how long it rests before
/// it lifts.
type SheetStroke = (&'static str, Vec<(egui::Vec2, f32)>);

/// Writes a picture of real strokes drawn through the canvas with the
/// brush engine, for a person to look at: rows slow, fast, light, heavy,
/// a tap, a flick and a lift while moving; columns a pen's packets (four
/// to a frame), the window's pointer events with a driver's pressure, and
/// a mouse. `SUE_STROKE_SHEET` names the PNG file.
#[test]
#[ignore = "writes a picture of the brush engine's strokes for a person to look at"]
fn engine_strokes_sheet() {
    let mut paint = new_engine("paint-engine-strokes-sheet");
    let ease = |s: f32| (s * std::f32::consts::PI).sin().max(0.0);
    let rows: Vec<SheetStroke> = vec![
        (
            "slow",
            (0..60)
                .map(|i| (vec2(2.5, 0.0), 0.15 + 0.5 * ease(i as f32 / 59.0)))
                .collect(),
        ),
        (
            "fast",
            (0..10)
                .map(|i| (vec2(15.0, 0.0), 0.2 + 0.5 * ease(i as f32 / 9.0)))
                .collect(),
        ),
        ("light", (0..30).map(|_| (vec2(5.0, 0.0), 0.12)).collect()),
        ("heavy", (0..30).map(|_| (vec2(5.0, 0.0), 0.95)).collect()),
        ("tap", vec![(vec2(0.0, 0.0), 0.5), (vec2(0.3, 0.0), 0.5)]),
        (
            "flick",
            (0..12)
                .map(|i| {
                    (
                        vec2(4.0 + i as f32 * 2.0, -0.5 * i as f32),
                        0.5 - 0.03 * i as f32,
                    )
                })
                .collect(),
        ),
        (
            "lift moving",
            (0..20).map(|_| (vec2(7.5, 0.0), 0.6)).collect(),
        ),
    ];
    let middle = paint.middle();
    let mut time = 20.0;
    for (row, (_, legs)) in rows.iter().enumerate() {
        for column in 0..3 {
            let start = middle + vec2(-330.0 + column as f32 * 230.0, -215.0 + row as f32 * 62.0);
            let pressure0 = legs.first().map_or(0.5, |leg| leg.1);
            let mut at = start;
            match column {
                0 => {
                    let mut events = vec![
                        egui::Event::PointerMoved(start),
                        button(start, true, egui::Modifiers::NONE),
                    ];
                    time += 0.005;
                    stylus::push_for_test(packet(
                        stylus::PenKind::Down,
                        start,
                        time,
                        Some(pressure0),
                    ));
                    for (by, pressure) in legs {
                        // Four packets to a frame.
                        for quarter in 1..=4 {
                            let p = at + *by * (quarter as f32 / 4.0);
                            time += 0.004;
                            stylus::push_for_test(packet(
                                stylus::PenKind::Move,
                                p,
                                time,
                                Some(*pressure),
                            ));
                        }
                        at += *by;
                        events.push(egui::Event::PointerMoved(at));
                        paint.step(std::mem::take(&mut events));
                    }
                    time += 0.004;
                    stylus::push_for_test(packet(stylus::PenKind::Up, at, time, None));
                    paint.step(vec![button(at, false, egui::Modifiers::NONE)]);
                }
                _ => {
                    let pen = column == 1;
                    if pen {
                        stylus::set_for_test(PenSource::WinTab, Some(pressure0));
                    }
                    paint.step(vec![egui::Event::PointerMoved(start)]);
                    paint.step(vec![button(start, true, egui::Modifiers::NONE)]);
                    for (by, pressure) in legs {
                        at += *by;
                        if pen {
                            stylus::set_for_test(PenSource::WinTab, Some(*pressure));
                        }
                        paint.step(vec![egui::Event::PointerMoved(at)]);
                    }
                    if pen {
                        stylus::set_for_test(PenSource::WinTab, None);
                    }
                    paint.step(vec![button(at, false, egui::Modifiers::NONE)]);
                    stylus::forget_for_test();
                }
            }
            paint.settle(3);
        }
    }
    let layers = paint.layers();
    let (_, width, bytes) = layers.last().unwrap();
    let height = bytes.len() / 4 / width;
    let mut out = Vec::with_capacity(width * height * 4);
    for pixel in bytes.chunks(4) {
        let cover = u32::from(pixel[3]);
        for &ink in &pixel[..3] {
            let ink = u32::from(ink);
            out.push(((ink * cover + 255 * (255 - cover)) / 255) as u8);
        }
        out.push(255);
    }
    let png = sue_io::encode_png_rgba(*width as u32, height as u32, &out).unwrap();
    let path = std::env::var("SUE_STROKE_SHEET").map_or_else(
        |_| std::env::temp_dir().join("sue-engine-strokes.png"),
        PathBuf::from,
    );
    std::fs::write(&path, png).unwrap();
    println!("engine strokes: {}", path.display());
}

/// The Blend tool on a layer's mask blends the mask: a hard edge the
/// eraser left goes soft, the layer's own pixels stay, and the drag is
/// one undo step (the Crash Course softens a mask so).
#[test]
fn sue_paint_the_blend_tool_softens_a_mask() {
    let mut paint = new_engine("paint-engine-blend-mask");
    paint.command("select.all");
    paint.command("edit.fill");
    paint.command("layer.mask.outside");
    paint.command("select.none");
    paint.command("layer.mask.edit");
    paint.command("tool.eraser");
    let middle = paint.middle();
    paint.slow(
        middle - vec2(150.0, 0.0),
        middle + vec2(150.0, 0.0),
        egui::Modifiers::NONE,
    );
    let layer = paint.layers().last().unwrap().2.clone();
    let (_, hard) = paint.mask().unwrap();
    let soft = |values: &[u8]| values.iter().filter(|&&v| v > 8 && v < 247).count();
    paint.command("tool.blend");
    for row in -2..=2 {
        let y = row as f32 * 6.0;
        paint.slow(
            middle - vec2(100.0, 40.0 - y),
            middle + vec2(100.0, 40.0 + y),
            egui::Modifiers::NONE,
        );
    }
    let (format, blended) = paint.mask().unwrap();
    assert_eq!(format, sue_core::PixelFormat::Gray8, "still a gray mask");
    assert!(
        soft(&blended) > soft(&hard) + 100,
        "the edge went soft: {} soft values, {} before",
        soft(&blended),
        soft(&hard)
    );
    assert_eq!(
        paint.layers().last().unwrap().2,
        layer,
        "the layer is untouched"
    );
    for _ in 0..5 {
        paint.command("edit.undo");
    }
    assert_eq!(paint.mask().unwrap().1, hard, "each drag was one undo step");
}

/// A vector layer brought in with a mask (a `.clip` file may hold one):
/// with Edit Mask on, the Eraser hides it in the mask's gray pixels, and
/// no line is added to the layer.
#[test]
fn sue_paint_a_vector_layers_mask_takes_strokes() {
    let mut paint = new_engine("paint-engine-vector-mask");
    paint.command("layer.new.vector");
    let layer = paint.top();
    assert!(
        sue_paint::vector::is_vector(&paint.runtime.engine.world.arena, layer),
        "a vector layer"
    );
    let (mut edit, _) =
        sue_paint::doc::new_mask(&paint.runtime.engine.world.arena, layer, |_, _| 1.0).unwrap();
    sue_core::ReversibleTransaction::apply(&mut edit, &mut paint.runtime.engine.world.arena)
        .unwrap();
    paint.settle(2);
    paint.command("layer.mask.edit");
    paint.command("tool.eraser");
    let middle = paint.middle();
    paint.slow(
        middle - vec2(120.0, 0.0),
        middle + vec2(120.0, 0.0),
        egui::Modifiers::NONE,
    );
    let (format, values) = paint.mask().unwrap();
    assert_eq!(format, sue_core::PixelFormat::Gray8);
    let hidden = values.iter().filter(|&&v| v < 128).count();
    assert!(hidden > 500, "the eraser hid a line of it: {hidden} px");
    paint.command("edit.undo");
    assert!(
        paint.mask().unwrap().1.iter().all(|&v| v == 255),
        "one undo step takes it back"
    );
}

/// With the layer's transparent pixels locked, a stroke across a line
/// recolors the line where it crosses (and adds nothing off it).
#[test]
fn sue_paint_locked_transparent_pixels_still_take_paint() {
    let mut paint = new_engine("paint-engine-lock-paints");
    let middle = paint.middle();
    paint.slow(
        middle - vec2(120.0, 0.0),
        middle + vec2(120.0, 0.0),
        egui::Modifiers::NONE,
    );
    let before = paint.layers().last().unwrap().2.clone();
    paint.command("layer.toggle.lock_alpha");
    // The sub color (white), so the crossing shows.
    paint.command("color.swap");
    paint.slow(
        middle - vec2(0.0, 100.0),
        middle + vec2(0.0, 100.0),
        egui::Modifiers::NONE,
    );
    let after = paint.layers().last().unwrap().2.clone();
    let alphas = |bytes: &[u8]| bytes.iter().skip(3).step_by(4).copied().collect::<Vec<_>>();
    assert_eq!(alphas(&after), alphas(&before), "no alpha changed");
    let recolored = before
        .chunks(4)
        .zip(after.chunks(4))
        .filter(|(a, b)| a[3] > 0 && a[..3] != b[..3])
        .count();
    assert!(
        recolored > 50,
        "the crossing took the new color: {recolored} px"
    );
}

/// The Blend tool on a layer with keyframes on is refused like a brush:
/// nothing changes and nothing goes on the undo stack.
#[test]
fn sue_paint_a_layer_with_keyframes_on_refuses_the_blend_tool() {
    let mut paint = new_engine("paint-engine-keyframes-blend");
    let middle = paint.middle();
    paint.slow(
        middle - vec2(120.0, 0.0),
        middle + vec2(120.0, 0.0),
        egui::Modifiers::NONE,
    );
    let layer = paint.top();
    let mut on = sue_paint::anim::motion::enable_keyframes(layer, true);
    sue_core::ReversibleTransaction::apply(&mut on, &mut paint.runtime.engine.world.arena).unwrap();
    let before = paint.layers().last().unwrap().2.clone();
    let depth = paint.runtime.engine.history.depth();
    paint.command("tool.blend");
    paint.slow(
        middle - vec2(0.0, 60.0),
        middle + vec2(0.0, 60.0),
        egui::Modifiers::NONE,
    );
    assert_eq!(paint.layers().last().unwrap().2, before, "nothing blended");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth,
        "nothing to undo"
    );
    assert!(
        paint.notice().to_lowercase().contains("keyframes"),
        "a notice says why: {:?}",
        paint.notice()
    );
}
