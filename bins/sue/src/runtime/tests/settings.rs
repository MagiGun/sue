//! Settings driven through real frames: pages opened by their deep
//! links, rows found where a frame drew them, never by a fixed position.

use super::*;

impl PaintDriver {
    /// Opens Settings at the page `id`.
    fn settings(&mut self, id: &str) -> Vec<(String, egui::Rect)> {
        self.runtime.request(
            ShellRequest::OpenSettings(Some(id.to_owned())),
            &mut NoTextures,
            self.now,
        );
        self.settle(8)
    }

    /// Types `text` into the Settings filter, which holds `was` (empty
    /// for none), in its place.
    fn filter(&mut self, was: &str, text: &str) -> Vec<(String, egui::Rect)> {
        let hint = sue_lang::text("settings", "filter-settings-ellipsis");
        let shown = if was.is_empty() { hint.as_str() } else { was };
        // The field: where its words are drawn, left of anything else
        // that says the same.
        let at = self
            .texts
            .iter()
            .filter(|(found, _)| found == shown)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("the filter field shows {shown:?}: {:?}", self.texts))
            .center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
        self.step(vec![key(egui::Key::Backspace, egui::Modifiers::NONE)]);
        self.step(vec![egui::Event::Text(text.to_owned())]);
        self.settle(6)
    }

    fn shows(&self, text: &str) -> bool {
        self.texts.iter().any(|(found, _)| found == text)
    }

    /// Turns the mouse wheel over the middle of Settings' page.
    fn scroll_settings(&mut self, by: f32) -> Vec<(String, egui::Rect)> {
        let at = self
            .runtime
            .shell
            .window_rect()
            .expect("Settings is open")
            .center()
            + vec2(150.0, 0.0);
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -by),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.settle(30)
    }
}

/// H-5: a key two commands hold is marked on both rows, with an offer to
/// take it from the other; once taken, the key runs the command it was
/// given to.
#[test]
fn a_key_held_twice_warns_and_works_once_taken_from_the_other() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("settings-keys");
    assert!(paint.shows("10.0\npx"), "the brush starts at 10 px");
    let texts = paint.settings("keymaps");
    assert!(
        texts.iter().any(|(text, _)| text == "Keyboard Shortcuts"),
        "{texts:?}"
    );
    // Bigger Brush, found by its name, gets R in place of ].
    paint.filter("", "Bigger Brush");
    assert!(paint.shows("Bigger Brush"), "{:?}", paint.texts);
    paint.click_text("]");
    assert!(paint.shows("Press keys..."), "{:?}", paint.texts);
    paint.step(vec![key(egui::Key::R, egui::Modifiers::NONE)]);
    paint.settle(6);
    // Recording the key did not also pick the Rotate tool.
    assert!(paint.shows("G-pen"), "{:?}", paint.texts);
    let held = "R also runs Rotate Tool; a press runs only one of them.";
    assert!(paint.shows(held), "{:?}", paint.texts);
    // The information line says so at once, with no window over it.
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("R is now a key of Bigger Brush")),
        "{:?}",
        paint.texts
    );
    // The other row is marked too.
    paint.filter("Bigger Brush", "Rotate Tool");
    assert!(
        paint.shows("R also runs Bigger Brush; a press runs only one of them."),
        "{:?}",
        paint.texts
    );
    // Take R from Rotate Tool: the marks go.
    paint.filter("Rotate Tool", "Bigger Brush");
    paint.click_text("Take R from Rotate Tool");
    paint.settle(4);
    assert!(!paint.shows(held), "{:?}", paint.texts);
    let rice = &paint.runtime.engine.world.rice;
    assert_eq!(
        rice.get_keybind("sue_paint_keys.tool_rotate")
            .map(|chords| chords.to_string()),
        Some(String::new())
    );
    // On the canvas, R makes the brush bigger and the pen stays.
    paint.runtime.shell.close_window();
    paint.settle(4);
    paint.step(vec![key(egui::Key::R, egui::Modifiers::NONE)]);
    paint.settle(6);
    assert!(paint.shows("12.0\npx"), "{:?}", paint.texts);
    assert!(paint.shows("G-pen"), "{:?}", paint.texts);
}

/// File, Preferences opens Settings on Sue Paint's page, in named groups;
/// Clip Studio's name for an option finds it.
#[test]
fn preferences_open_sue_paints_page_of_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("settings-preferences");
    paint.command("file.preferences");
    paint.settle(8);
    for title in ["Sue Paint", "NEW CANVAS", "PEN AND TOOLS", "Resolution"] {
        assert!(paint.shows(title), "{title}: {:?}", paint.texts);
    }
    // Clip Studio's name for the cursor option finds Sue's.
    paint.filter("", "Brush-type cursor");
    assert!(paint.shows("Brush cursor"), "{:?}", paint.texts);
    assert!(!paint.shows("Rotation step"), "{:?}", paint.texts);
    paint.runtime.shell.close_window();
}

/// Rows most people never need wait under a closed title, which opens on
/// a click; the custom colors show only while the theme is Custom.
#[test]
fn advanced_groups_start_closed_and_open_on_a_click() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("settings-advanced");
    paint.settings("appearance");
    paint.scroll_settings(3000.0);
    assert!(paint.shows("Sizes and Spacing"), "{:?}", paint.texts);
    assert!(!paint.shows("Border thickness"), "{:?}", paint.texts);
    paint.click_text("Sizes and Spacing");
    assert!(paint.shows("Border thickness"), "{:?}", paint.texts);
    // Sue Paint's custom colors wait for its Custom theme (scrolled to the
    // bottom of the page, their last row shows).
    paint.settings(sue_paint::ID);
    paint.scroll_settings(3000.0);
    assert!(
        !paint.shows("Shadow under floating surfaces"),
        "{:?}",
        paint.texts
    );
    paint
        .runtime
        .engine
        .world
        .rice
        .set(
            "sue_paint.theme",
            sue_rice::RiceValue::Text("custom".to_owned()),
        )
        .unwrap();
    paint.settle(4);
    paint.scroll_settings(3000.0);
    assert!(
        paint.shows("Shadow under floating surfaces"),
        "{:?}",
        paint.texts
    );
    paint.runtime.shell.close_window();
}
