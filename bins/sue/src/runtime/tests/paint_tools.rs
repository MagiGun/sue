//! Sue Paint's tool palettes: the Tool palette, Sub Tool, Tool Property,
//! Brush Size and Sub Tool Detail, driven through real frames.

use super::*;

/// Gives sub tool command `command` the chord `chord`, as Settings does
/// for a command without a default key.
fn bind_sub_tool(driver: &mut PaintDriver, command: &str, chord: &str) {
    let section = "sue_paint_keys";
    let rice = &mut driver.runtime.engine.world.rice;
    let (setting, value) = sue_wm::shortcuts::bind(rice, section, command, &chord.parse().unwrap());
    rice.set(&setting, value).unwrap();
}

#[test]
fn a_key_given_to_a_sub_tool_picks_it_and_its_tool() {
    let mut driver = PaintDriver::new("paint-tools-sub-key");
    // A fresh canvas is on the Pen: Sketch's sub tools are not listed.
    let texts = driver.settle(4);
    assert!(
        !texts.iter().any(|(text, _)| text == "Mechanical pencil"),
        "{texts:?}"
    );
    bind_sub_tool(&mut driver, "subtool.pencil.Mechanical_pencil", "Shift+F9");
    driver.step(vec![key(egui::Key::F9, egui::Modifiers::SHIFT)]);
    let texts = driver.settle(4);
    assert!(
        texts.iter().any(|(text, _)| text == "Mechanical pencil"),
        "the key picked the Sketch tool: {texts:?}"
    );
    // The same command runs from Auto Actions and scripts.
    driver.command("subtool.pen.Turnip_pen");
    let texts = driver.settle(4);
    assert!(
        texts.iter().any(|(text, _)| text == "Turnip pen"),
        "{texts:?}"
    );
}
