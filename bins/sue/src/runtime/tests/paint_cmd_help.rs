//! The Help group of Sue Paint's commands, each run through the runtime
//! on a canvas as its menu item runs it.

use super::*;

/// Runs `command` on the canvas; what the frames after draw.
fn run(paint: &mut PaintDriver, command: &str) -> Vec<(String, egui::Rect)> {
    paint.command(command);
    paint.settle(4)
}

/// Closes what a command opened over the canvas.
fn close(paint: &mut PaintDriver) {
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(4);
}

fn drawn(texts: &[(String, egui::Rect)], wanted: &str) -> bool {
    texts.iter().any(|(text, _)| text.contains(wanted))
}

#[test]
fn every_help_command_runs_on_a_canvas() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("paint-cmd-help");
    let registry = sue_paint::registry();
    let group: Vec<&str> = registry
        .iter()
        .filter(|command| command.group == "help")
        .map(|command| command.name)
        .collect();
    assert_eq!(
        group,
        ["help.guide", "help.shortcuts", "help.updates", "help.about"]
    );

    // The guide opens over the canvas.
    let texts = run(&mut paint, "help.guide");
    assert!(drawn(&texts, "Sue Paint Guide"), "{texts:?}");
    assert!(drawn(&texts, "Starting"), "{texts:?}");
    close(&mut paint);

    // The list of keys and gestures.
    let texts = run(&mut paint, "help.shortcuts");
    assert!(drawn(&texts, "Keyboard Shortcuts"), "{texts:?}");
    close(&mut paint);

    // Where new versions are found.
    let texts = run(&mut paint, "help.updates");
    assert!(drawn(&texts, "Check for Updates"), "{texts:?}");
    assert!(
        drawn(&texts, "https://github.com/MagiGun/sue/releases"),
        "{texts:?}"
    );
    close(&mut paint);

    // About is a notice.
    run(&mut paint, "help.about");
    let (message, error) = paint.runtime.shell.message().expect("a notice");
    assert!(
        message.starts_with("Sue Paint: layered painting"),
        "{message}"
    );
    assert!(!error);
}
