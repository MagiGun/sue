//! Sue Paint's tool and brush keys (the command group `tool`), run
//! through the runtime as menus, Auto Actions and keys run them.

use super::*;
use sue_paint::tools::Tool;

/// `true` while the shell says a command is not in Sue Paint yet.
fn not_yet(driver: &PaintDriver) -> bool {
    driver.runtime.shell.message().is_some_and(|(text, _)| {
        text.contains("not in Sue Paint yet") || text.contains("arrives in a later")
    })
}

/// The name the Sub Tool palette shows for the sub tool a fresh studio
/// has picked on `tool`.
fn first_sub(tool: Tool) -> String {
    tool.sub_tools()
        .first()
        .map(|sub| sub.name.clone())
        .expect("every tool has a sub tool")
}

/// Whether the last frame drew `text`.
fn shows(texts: &[(String, egui::Rect)], text: &str) -> bool {
    texts.iter().any(|(found, _)| found == text)
}

#[test]
fn every_command_of_the_tool_group_runs_on_a_canvas() {
    let mut driver = PaintDriver::new("paint-cmd-tool");
    let registry = sue_paint::registry();
    let group: Vec<&str> = registry
        .iter()
        .filter(|command| command.group == "tool")
        .map(|command| command.name)
        .collect();
    assert_eq!(group.len(), 22, "{group:?}");
    for name in &group {
        driver.command(name);
        driver.settle(4);
        assert!(
            !not_yet(&driver),
            "{name}: {:?}",
            driver.runtime.shell.message()
        );
        // Nothing of the group opens anything; Esc would close it.
        driver.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        driver.settle(2);
    }
}

#[test]
fn the_tool_commands_pick_their_tools_and_the_palette_shows_them() {
    let mut driver = PaintDriver::new("paint-cmd-tool-picks");
    // Each command, from a tool of another key, picks the first tool of
    // its key; the Sub Tool palette then lists that tool's sub tools.
    let cases = [
        ("tool.eraser", Tool::Eraser),
        ("tool.pen", Tool::Pen),
        ("tool.brush", Tool::Brush),
        ("tool.fill", Tool::Fill),
        ("tool.figure", Tool::Figure),
        ("tool.text", Tool::Text),
        ("tool.selection", Tool::Select),
        ("tool.auto_select", Tool::AutoSelect),
        ("tool.eyedropper", Tool::Eyedropper),
        ("tool.blend", Tool::Blend),
        ("tool.correct_line", Tool::CorrectLine),
        ("tool.operation", Tool::Operation),
        ("tool.move_layer", Tool::MoveLayer),
        ("tool.zoom", Tool::Zoom),
        ("tool.hand", Tool::Hand),
    ];
    for (command, tool) in cases {
        driver.command(command);
        let texts = driver.settle(4);
        let sub = first_sub(tool);
        assert!(shows(&texts, &sub), "{command}: {sub:?} in {texts:?}");
    }
    // A second press of a shared key goes on to the next tool of it.
    driver.command("tool.pen");
    let texts = driver.settle(4);
    assert!(shows(&texts, &first_sub(Tool::Pen)));
    driver.command("tool.pen");
    let texts = driver.settle(4);
    let pencil = first_sub(Tool::Pencil);
    assert!(shows(&texts, &pencil), "{pencil:?} in {texts:?}");
}

#[test]
fn the_keys_run_the_group_as_the_commands_do() {
    let mut driver = PaintDriver::new("paint-cmd-tool-keys");
    let plain = egui::Modifiers::NONE;
    // E picks the eraser by its key, not only by its command.
    driver.step(vec![key(egui::Key::E, plain)]);
    let texts = driver.settle(4);
    let eraser = first_sub(Tool::Eraser);
    assert!(shows(&texts, &eraser), "{eraser:?} in {texts:?}");
    // ] and [ step the brush size the Tool Property palette shows.
    let size = |texts: &[(String, egui::Rect)]| {
        texts
            .iter()
            .find_map(|(text, _)| text.strip_suffix("\npx")?.parse::<f32>().ok())
            .expect("the brush size is shown")
    };
    let before = size(&texts);
    driver.step(vec![key(egui::Key::CloseBracket, plain)]);
    let bigger = size(&driver.settle(4));
    assert!(bigger > before, "{before} to {bigger}");
    driver.step(vec![key(egui::Key::OpenBracket, plain)]);
    let back = size(&driver.settle(4));
    assert!((back - before).abs() < 0.05, "{before} back to {back}");
    // X and C run without a word from the shell.
    for pressed in [egui::Key::X, egui::Key::C, egui::Key::C] {
        driver.step(vec![key(pressed, plain)]);
        driver.settle(2);
        assert!(!not_yet(&driver), "{pressed:?}");
    }
}
