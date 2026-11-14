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
    assert_eq!(names.len(), 35, "{names:?}");
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
        // Only Paper changes the picture.
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
