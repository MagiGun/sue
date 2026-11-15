//! Sue Paint: the symmetry and perspective rulers.

use super::*;

#[test]
fn sue_paint_symmetry_ruler_mirrors_strokes_until_turned_off() {
    let (mut runtime, root) = runtime("paint-symmetry");
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
        now += Duration::from_millis(8);
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
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    let ink = |runtime: &Runtime, columns: std::ops::Range<u32>, rows: std::ops::Range<u32>| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        let bytes = raster.bytes().unwrap();
        rows.flat_map(|y| columns.clone().map(move |x| (x, y)))
            .map(|(x, y)| u32::from(bytes[((y * raster.width() + x) * 4 + 3) as usize]))
            .sum::<u32>()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    // U three times picks the Ruler tool and comma steps back round to
    // the Symmetrical Ruler; a click lays a vertical ruler.
    for _ in 0..3 {
        step(&mut runtime, vec![key(egui::Key::U, plain)]);
    }
    step(&mut runtime, vec![key(egui::Key::Comma, plain)]);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(middle), press(middle, true)],
    );
    step(&mut runtime, vec![press(middle, false)]);
    // A pen stroke on the left only.
    step(&mut runtime, vec![key(egui::Key::P, plain)]);
    let mut stroke = |runtime: &mut Runtime, y: f32| {
        let (a, b) = (middle + vec2(-80.0, y), middle + vec2(-30.0, y));
        step(runtime, vec![egui::Event::PointerMoved(a), press(a, true)]);
        step(
            runtime,
            vec![
                egui::Event::PointerMoved(middle + vec2(-55.0, y)),
                egui::Event::PointerMoved(b),
            ],
        );
        step(runtime, vec![press(b, false)]);
    };
    stroke(&mut runtime, 0.0);
    assert!(ink(&runtime, 650..740, 580..620) > 0, "drawn on the left");
    assert!(
        ink(&runtime, 860..950, 580..620) > 0,
        "mirrored on the right"
    );
    // Snap to Special Ruler (Ctrl+2) turns it off: the next stroke
    // stays on its side.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint.clone(),
            command: "view.snap_special".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    stroke(&mut runtime, 60.0);
    assert!(ink(&runtime, 650..740, 690..760) > 0);
    assert_eq!(
        ink(&runtime, 860..950, 690..760),
        0,
        "no mirror with the ruler off"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_perspective_rulers_hold_strokes_and_take_new_vanishing_points() {
    let (mut runtime, root) = runtime("paint-perspective");
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
        now += Duration::from_millis(8);
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
    let points = |runtime: &Runtime| {
        sue_paint::rulers::Rulers::of(&runtime.engine.world.arena, picture)
            .perspective
            .map_or(0, |ruler| ruler.points.len())
    };
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    // Layer, Ruler/Frame, Create Perspective Ruler: 2-point by default.
    command(&mut runtime, "layer.ruler.perspective");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    assert_eq!(points(&runtime), 2);
    // The ruler lands on a layer of its own (FGV-2): strokes go there.
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    // How far across the ink on the layer reaches.
    let width = |runtime: &Runtime| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        let (w, bytes) = (raster.width(), raster.bytes().unwrap());
        let columns: Vec<u32> = (0..raster.height())
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|(x, y)| bytes[((y * w + x) * 4 + 3) as usize] > 0)
            .map(|(x, _)| x)
            .collect();
        columns.iter().max().unwrap() - columns.iter().min().unwrap()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let drag = |runtime: &mut Runtime,
                step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                from: egui::Pos2,
                to: egui::Pos2| {
        step(
            runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        for part in 1..=8 {
            let at = from + (to - from) * (part as f32 / 8.0);
            step(runtime, vec![egui::Event::PointerMoved(at)]);
        }
        step(runtime, vec![press(to, false)]);
        let _ = runtime;
    };
    // A pen stroke heading down but drifting right runs straight up
    // and down, the upright of two-point perspective.
    let middle = runtime.shell.page_rect().center();
    let (top, bottom) = (middle + vec2(-20.0, -150.0), middle + vec2(10.0, 10.0));
    step(&mut runtime, vec![key(egui::Key::P, plain)]);
    drag(&mut runtime, &mut step, top, bottom);
    let snapped = width(&runtime);
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    // With Snap to Special Ruler off it drifts as drawn.
    command(&mut runtime, "view.snap_special");
    drag(&mut runtime, &mut step, top, bottom);
    let free = width(&runtime);
    assert!(snapped * 2 < free, "{snapped} against {free}");
    command(&mut runtime, "view.snap_special");
    // Back to the Ruler tool, on the Perspective Ruler that Create
    // Perspective Ruler picked: two lines that meet make a third
    // vanishing point.
    for _ in 0..3 {
        step(&mut runtime, vec![key(egui::Key::U, plain)]);
    }
    drag(
        &mut runtime,
        &mut step,
        middle + vec2(-200.0, -100.0),
        middle + vec2(-100.0, -60.0),
    );
    assert_eq!(points(&runtime), 2, "one line is not a point yet");
    drag(
        &mut runtime,
        &mut step,
        middle + vec2(-200.0, 100.0),
        middle + vec2(-100.0, 60.0),
    );
    assert_eq!(points(&runtime), 3);
    // Delete Ruler takes every ruler away, and undo brings them back.
    command(&mut runtime, "layer.ruler.delete");
    assert_eq!(points(&runtime), 0);
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(points(&runtime), 3);
    let _ = std::fs::remove_dir_all(&root);
}
