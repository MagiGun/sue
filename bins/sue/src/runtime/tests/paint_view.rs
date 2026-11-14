//! Sue Paint: the View menu's commands, through the shell.

use super::*;

/// Opens Sue Paint and makes a picture with Ctrl+N; the frame clock and
/// the picture.
fn paint_with_picture(runtime: &mut Runtime, ctx: &egui::Context) -> (Instant, sue_core::EntityId) {
    let mut now = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(runtime, ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    for events in [
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        Vec::new(),
    ] {
        frame_with(runtime, ctx, now, events);
        now += Duration::from_millis(8);
    }
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .expect("Ctrl+N made a picture");
    (now, picture)
}

#[test]
fn sue_paint_runs_every_view_command() {
    let (mut runtime, root) = runtime("paint-view-all");
    let ctx = egui::Context::default();
    let (mut now, picture) = paint_with_picture(&mut runtime, &ctx);
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let names: Vec<&str> = sue_paint::registry()
        .iter()
        .filter(|command| command.group == "view")
        .map(|command| command.name)
        .collect();
    assert_eq!(names.len(), 42, "{names:?}");
    let paper = |runtime: &Runtime| {
        sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .find(|info| info.kind == sue_model::picture::LayerKind::Paper)
            .map(|info| info.style.visible)
    };
    assert_eq!(paper(&runtime), Some(true));
    for name in &names {
        let depth = runtime.engine.history.depth();
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: (*name).to_owned(),
            },
            &mut NoTextures,
            now,
        );
        frame_with(&mut runtime, &ctx, now, Vec::new());
        now += Duration::from_millis(8);
        assert!(
            !runtime.shell.message().is_some_and(|(text, _)| {
                text.contains("not in Sue Paint yet") || text.contains("arrives in a later")
            }),
            "{name}: {:?}",
            runtime.shell.message()
        );
        // Only Paper changes the picture (and Toggle Special Ruler
        // Snapping, which a picture without a special ruler grays).
        let steps = usize::from(*name == "view.paper");
        assert_eq!(runtime.engine.history.depth(), depth + steps, "{name}");
        // Esc closes what opened (Preview Settings).
        frame_with(
            &mut runtime,
            &ctx,
            now,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        now += Duration::from_millis(8);
    }
    assert_eq!(paper(&runtime), Some(false), "View, Paper hid the paper");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ctrl_4_turns_the_pictures_special_ruler_snapping_off_and_undo_back_on() {
    let (mut runtime, root) = runtime("paint-view-special-snap");
    let ctx = egui::Context::default();
    let (mut now, picture) = paint_with_picture(&mut runtime, &ctx);
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
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
    // The Symmetrical Ruler, laid upright through the middle.
    for _ in 0..3 {
        step(&mut runtime, vec![key(egui::Key::U, plain)]);
    }
    step(&mut runtime, vec![key(egui::Key::Comma, plain)]);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(middle), press(middle, true)],
    );
    step(&mut runtime, vec![press(middle, false)]);
    step(&mut runtime, vec![key(egui::Key::P, plain)]);
    let stroke =
        |step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>), runtime: &mut Runtime, y: f32| {
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
    let snaps = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .get_by_id(picture)
            .unwrap()
            .property("ruler.special.snap")
            .is_none()
    };
    assert!(snaps(&runtime));
    stroke(&mut step, &mut runtime, 0.0);
    assert!(ink(&runtime, 860..950, 580..620) > 0, "mirrored");
    // Ctrl+4: the picture's switch goes off, one step of its own; the
    // next stroke stays on its side.
    let depth = runtime.engine.history.depth();
    step(
        &mut runtime,
        vec![key(egui::Key::Num4, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, Vec::new());
    assert!(!snaps(&runtime), "Ctrl+4 switched it off");
    assert_eq!(runtime.engine.history.depth(), depth + 1);
    stroke(&mut step, &mut runtime, 60.0);
    assert!(ink(&runtime, 650..740, 640..1100) > 0);
    assert_eq!(ink(&runtime, 860..950, 640..1100), 0, "no mirror");
    // Undo takes the stroke back, then the switch.
    for _ in 0..2 {
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
    }
    step(&mut runtime, Vec::new());
    assert!(snaps(&runtime), "undone, it snaps again");
    assert_eq!(ink(&runtime, 650..740, 640..1100), 0, "the stroke undone");
    stroke(&mut step, &mut runtime, 120.0);
    assert!(ink(&runtime, 860..950, 640..1100) > 0, "mirrored once more");
    let _ = std::fs::remove_dir_all(&root);
}
