//! Sue Paint: every command of the Animation group, run through the
//! runtime, each that changes the picture undone and redone (rule 7).

use super::*;

/// The arena as bytes (every entity, id and relation, never revisions),
/// to compare before and after.
fn arena_bytes(runtime: &Runtime) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

#[test]
fn every_animation_command_runs_and_each_change_undoes_and_redoes() {
    let (mut runtime, root) = runtime("paint-cmd-anim");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    let paint = AppletId::new(sue_paint::ID).unwrap();
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
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
        .expect("a canvas");

    let registry = sue_paint::registry();
    let group: Vec<&str> = registry
        .iter()
        .filter(|command| command.group == "anim")
        .map(|command| command.name)
        .collect();
    assert!(group.len() >= 38, "{group:?}");
    // Twice: the first round makes the timeline, folders and cels the
    // second round's commands act on.
    let mut changed = std::collections::BTreeSet::new();
    for round in 0..2 {
        for &name in &group {
            let before = arena_bytes(&runtime);
            let depth = runtime.engine.history.depth();
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: name.to_owned(),
                },
                &mut NoTextures,
                start,
            );
            step(&mut runtime, Vec::new());
            let message = runtime.shell.message().map(|(text, _)| text.to_owned());
            assert!(
                !message.as_deref().is_some_and(|text| {
                    text.contains("not in Sue Paint yet") || text.contains("arrives in a later")
                }),
                "{name}: {message:?}"
            );
            if runtime.engine.history.depth() > depth {
                assert_eq!(
                    runtime.engine.history.depth(),
                    depth + 1,
                    "{name} is one step"
                );
                changed.insert(name);
                let after = arena_bytes(&runtime);
                runtime
                    .engine
                    .history
                    .undo(&mut runtime.engine.world)
                    .expect("undo");
                assert!(
                    arena_bytes(&runtime) == before,
                    "{name}: undo, round {round}"
                );
                runtime
                    .engine
                    .history
                    .redo(&mut runtime.engine.world)
                    .expect("redo");
                assert!(
                    arena_bytes(&runtime) == after,
                    "{name}: redo, round {round}"
                );
            }
            // Close whatever opened, and stop playing.
            step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
            step(&mut runtime, Vec::new());
        }
    }
    // The commands that change the picture did, and were undone.
    for name in [
        "anim.timeline.new",
        "anim.layer.folder",
        "anim.layer.camera",
        "anim.cel.new",
        "anim.cel.assign",
        "anim.frame.insert",
        "anim.frame.delete",
    ] {
        assert!(
            changed.contains(name),
            "{name} changed nothing: {changed:?}"
        );
    }
    assert!(sue_paint::anim::timeline(&runtime.engine.world.arena, picture).is_some());
    let _ = std::fs::remove_dir_all(&root);
}

/// The names of the cels the first track shows, by the frame each key is
/// on.
fn shown(runtime: &Runtime, picture: sue_core::EntityId) -> Vec<(u32, String)> {
    let arena = &runtime.engine.world.arena;
    let sequence = sue_paint::anim::timeline(arena, picture).expect("a timeline");
    sue_paint::anim::keys(&sequence.tracks[0], sequence.rate)
        .into_iter()
        .filter_map(|(frame, cel)| {
            let name = arena.get_by_id(cel?).ok()?.name().to_owned();
            Some((frame, name))
        })
        .collect()
}

#[test]
fn cels_are_renamed_in_timeline_and_layer_order_and_select_next_cel_takes_a_key() {
    let (mut runtime, root) = runtime("paint-cmd-anim-rename");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    let paint = AppletId::new(sue_paint::ID).unwrap();
    runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, now);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
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
        .expect("a canvas");
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };

    // Cels 1, 2 and 3 on frames 1, 3 and 5, then an in-between on frame
    // 2: it is named 4, after the highest.
    command(&mut runtime, "anim.layer.folder");
    for frame in 0..3 {
        if frame > 0 {
            command(&mut runtime, "anim.next");
            command(&mut runtime, "anim.next");
        }
        command(&mut runtime, "anim.cel.new");
    }
    command(&mut runtime, "anim.first");
    command(&mut runtime, "anim.next");
    command(&mut runtime, "anim.cel.new");
    let made = |names: [&str; 4]| -> Vec<(u32, String)> {
        [0, 1, 2, 4]
            .into_iter()
            .zip(names)
            .map(|(frame, name)| (frame, name.to_owned()))
            .collect()
    };
    assert_eq!(shown(&runtime, picture), made(["1", "4", "2", "3"]));

    // Rename in Timeline Order: 1, 2, 3, 4 along the track, one step.
    let depth = runtime.engine.history.depth();
    command(&mut runtime, "anim.cel.rename_timeline");
    assert_eq!(shown(&runtime, picture), made(["1", "2", "3", "4"]));
    assert_eq!(runtime.engine.history.depth(), depth + 1);
    runtime
        .engine
        .history
        .undo(&mut runtime.engine.world)
        .expect("undo");
    assert_eq!(shown(&runtime, picture), made(["1", "4", "2", "3"]));
    runtime
        .engine
        .history
        .redo(&mut runtime.engine.world)
        .expect("redo");
    assert_eq!(shown(&runtime, picture), made(["1", "2", "3", "4"]));
    // Again: nothing left to rename, so no step.
    command(&mut runtime, "anim.cel.rename_timeline");
    assert_eq!(runtime.engine.history.depth(), depth + 1);

    // Rename in Layer Order: the bottom cel (made first) is 1, the
    // in-between (made last, on top) is 4.
    command(&mut runtime, "anim.cel.rename_layer");
    assert_eq!(shown(&runtime, picture), made(["1", "4", "2", "3"]));
    assert_eq!(runtime.engine.history.depth(), depth + 2);

    // Select Next Cel ships with no key; given 2 in Settings, 2 runs it
    // (A25-5): from frame 1 it goes to the next key, frame 2, where
    // Delete Keyframe then acts.
    let rice = &runtime.engine.world.rice;
    let (setting, value) = sue_wm::shortcuts::bind(
        rice,
        "sue_paint_keys",
        "anim.cel.next",
        &"2".parse().expect("a chord"),
    );
    runtime
        .engine
        .world
        .rice
        .set(&setting, value)
        .expect("the key setting takes it");
    command(&mut runtime, "anim.first");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Num2, plain)]);
    step(&mut runtime, Vec::new());
    command(&mut runtime, "anim.key.delete");
    let frames: Vec<u32> = shown(&runtime, picture)
        .into_iter()
        .map(|(frame, _)| frame)
        .collect();
    assert_eq!(frames, [0, 2, 4], "the key on frame 2 went");
    let _ = std::fs::remove_dir_all(&root);
}
