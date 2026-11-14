//! Sue Paint's Window commands, run through the runtime as the menus and
//! the keys run them (P3-1, the Window group).

use super::*;

/// The names a test runs for the Window group: every entry, a family as
/// its first member.
fn window_commands() -> Vec<(String, sue_wm::Flags)> {
    sue_paint::registry()
        .iter()
        .filter(|command| command.group == "window")
        .map(|command| {
            let name = if command.is_family() {
                format!("{}0", command.name)
            } else {
                command.name.to_owned()
            };
            (name, command.flags)
        })
        .collect()
}

/// The error the status line shows, if any.
fn error_shown(driver: &PaintDriver) -> Option<String> {
    driver
        .runtime
        .shell
        .message()
        .filter(|(_, error)| *error)
        .map(|(text, _)| text.to_owned())
}

#[test]
fn sue_paint_runs_every_window_command_without_touching_the_picture() {
    let mut driver = PaintDriver::new("paint-window-every");
    let commands = window_commands();
    assert!(commands.len() >= 40, "{}", commands.len());
    let entities = driver.runtime.engine.world.arena.len();
    let steps = driver.runtime.engine.history.len();
    // Those that leave the canvas (New Window, the interface) come last,
    // so each of the others runs on the one canvas as it was.
    let (stay, leave): (Vec<_>, Vec<_>) = commands
        .into_iter()
        .partition(|(_, flags)| !flags.has(sue_wm::Flags::LEAVES_CANVAS));
    for (name, _) in stay.iter().chain(&leave) {
        driver.command(name);
        driver.settle(3);
        assert_eq!(error_shown(&driver), None, "{name}");
        let message = driver.runtime.shell.message().map(|(text, _)| text);
        assert!(
            !message.is_some_and(|text| text.contains("not in Sue Paint yet")),
            "{name}: {message:?}"
        );
        // Close whatever opened (the workspace dialogs).
        driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        driver.settle(2);
        assert_eq!(
            driver.runtime.engine.world.arena.len(),
            entities,
            "{name} made or removed an entity"
        );
        if !name.starts_with("window.interface.") {
            assert_eq!(
                driver.runtime.engine.history.len(),
                steps,
                "{name} left an undo step"
            );
        }
    }
    // Back to the desktop interface for whatever runs after.
    driver.command("window.interface.desktop");
    driver.settle(3);
}

#[test]
fn sue_paint_hide_all_palettes_hides_them_and_brings_them_back() {
    let mut driver = PaintDriver::new("paint-window-hide-all");
    let shown = driver.settle(3);
    assert!(
        shown.iter().any(|(text, _)| text == "Navigator"),
        "the Navigator palette is shown at first"
    );
    driver.command("window.hide_all");
    let hidden = driver.settle(4);
    assert!(
        !hidden.iter().any(|(text, _)| text == "Navigator"),
        "Hide All Palettes hides the Navigator"
    );
    assert!(hidden.len() < shown.len(), "fewer words on screen");
    driver.command("window.hide_all");
    let back = driver.settle(4);
    assert!(
        back.iter().any(|(text, _)| text == "Navigator"),
        "a second Hide All Palettes brings them back"
    );
}

#[test]
fn sue_paint_new_window_and_the_canvas_list_move_between_views() {
    let mut driver = PaintDriver::new("paint-window-canvases");
    driver.command("window.new");
    driver.settle(4);
    // Two views of one picture now: Previous Canvas, Next Canvas and the
    // list of canvases all go between them without an error.
    for name in [
        "window.prev_tab",
        "window.next_tab",
        "window.canvas.0",
        "window.canvas.1",
        "window.cascade",
        "window.tile",
    ] {
        driver.command(name);
        driver.settle(3);
        assert_eq!(error_shown(&driver), None, "{name}");
    }
}
