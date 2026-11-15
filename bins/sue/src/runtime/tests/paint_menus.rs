//! Sue Paint's menus and keys in the running app: the keys the menus show
//! reach the studio, Tab stays with Sue Paint, and the Window menu reads
//! the palettes' titles.

use super::*;

fn press(key_name: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![key(key_name, modifiers)]
}

#[test]
fn tab_hides_the_palettes_in_sue_paint_and_never_opens_the_command_palette() {
    let mut paint = PaintDriver::new("paint-menus-tab");
    // The Navigator palette is docked from the start.
    paint.settle(4);
    assert!(paint.rect_of("Navigator").is_some(), "{:?}", paint.texts);
    // Tab: Hide All Palettes, as Clip Studio Paint has it.
    paint.step(press(egui::Key::Tab, egui::Modifiers::NONE));
    paint.settle(30);
    assert!(
        !paint.runtime.shell.palette_open(),
        "Tab stayed with Sue Paint"
    );
    assert!(paint.rect_of("Navigator").is_none(), "the palettes hid");
    // Tab again shows them.
    paint.step(press(egui::Key::Tab, egui::Modifiers::NONE));
    paint.settle(30);
    assert!(!paint.runtime.shell.palette_open());
    assert!(
        paint.rect_of("Navigator").is_some(),
        "the palettes came back"
    );
    // The slash is Clip Studio Paint's Zoom tool inside Sue Paint, so it
    // does not open the command palette there either.
    paint.step(press(egui::Key::Slash, egui::Modifiers::NONE));
    paint.settle(4);
    assert!(
        !paint.runtime.shell.palette_open(),
        "/ stayed with Sue Paint"
    );
}

#[test]
fn the_settings_keys_open_shortcut_settings_and_modifier_key_settings() {
    let mut paint = PaintDriver::new("paint-menus-settings-keys");
    assert!(paint.runtime.shell.window_rect().is_none());
    // Ctrl+Shift+Alt+K: Shortcut Settings, Sue's Settings on its keys.
    let all = egui::Modifiers {
        alt: true,
        ctrl: true,
        shift: true,
        mac_cmd: false,
        command: true,
    };
    paint.step(press(egui::Key::K, all));
    paint.settle(30);
    assert!(
        paint.runtime.shell.window_rect().is_some(),
        "Shortcut Settings opened"
    );
    paint.runtime.shell.dismiss_overlays();
    paint.settle(30);
    // Ctrl+Shift+Alt+Y: Modifier Key Settings, the studio's own dialog.
    paint.step(press(egui::Key::Y, all));
    let texts = paint.settle(10);
    assert!(
        texts
            .iter()
            .any(|(text, _)| text.starts_with("Modifier Key Settings")),
        "Modifier Key Settings opened: {texts:?}"
    );
}

#[test]
fn shift_delete_deletes_outside_the_selection_and_alt_delete_fills() {
    let mut paint = PaintDriver::new("paint-menus-shift-delete");
    let depth = paint.runtime.engine.history.depth();
    // Without a selection Delete Outside Selection is grayed, so
    // Shift+Delete does nothing (it used to fill the whole layer).
    paint.step(press(egui::Key::Delete, egui::Modifiers::SHIFT));
    paint.settle(4);
    assert_eq!(paint.runtime.engine.history.depth(), depth);
    // Alt+Delete fills.
    paint.step(press(egui::Key::Delete, egui::Modifiers::ALT));
    paint.settle(4);
    assert_eq!(paint.runtime.engine.history.depth(), depth + 1, "filled");
}
