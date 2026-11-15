//! Sue Paint: the command bar, Quick Access and their settings (gaps
//! WS-2, WS-3, WS-4, WS-7, WS-8, CT-31, FG-36, FG-38 and FGV-55 of the
//! 0.0.8 plan). Buttons are found by the name the frame tells assistive
//! technology and clicked where that frame drew them, never at fixed
//! places; what a change keeps is read back from the bar's own files.

use super::*;
use egui::{Modifiers, PointerButton, Rect};

/// Sue Paint at 1440 by 900 with a canvas open, its palettes as its
/// layout style lays them out, driven a frame at a time; each frame says
/// every control's name, where it is and whether it is on.
struct Bar {
    driver: PaintDriver,
    /// The controls of the last frame: name, rect, on.
    nodes: Vec<(String, Rect, Option<bool>)>,
    /// The controls of the last frame that cannot be pressed now.
    disabled: Vec<(String, Rect)>,
    /// The keys held.
    held: Modifiers,
}

impl Bar {
    fn new(tag: &str) -> Self {
        let driver = PaintDriver::fresh(tag);
        driver.ctx.enable_accesskit();
        let mut bar = Self {
            driver,
            nodes: Vec::new(),
            disabled: Vec::new(),
            held: Modifiers::NONE,
        };
        bar.settle(4);
        bar
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let driver = &mut self.driver;
        // egui keeps the keys held from frame to frame through this event.
        let mut all = vec![egui::Event::ModifiersChanged(self.held)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events: all,
            time: Some(driver.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        driver.now += Duration::from_millis(16);
        let runtime = &mut driver.runtime;
        let now = driver.now;
        let output = driver
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        driver.texts = text_rects(&output);
        self.disabled = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter(|(_, node)| node.is_disabled())
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                let rect = Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                );
                Some((node.label()?.to_owned(), rect))
            })
            .collect();
        self.nodes = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| {
                let label = node.label()?;
                let bounds = node.bounds()?;
                let rect = Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                );
                let on = node
                    .toggled()
                    .map(|toggled| toggled == egui::accesskit::Toggled::True);
                Some((label.to_owned(), rect, on))
            })
            .collect();
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// Where the control `name` is, the topmost then the leftmost of those
    /// in `area`.
    fn node_in(&self, name: &str, area: Rect) -> Option<Rect> {
        self.nodes
            .iter()
            .filter(|(label, rect, _)| label == name && area.contains(rect.center()))
            .map(|(_, rect, _)| *rect)
            .min_by(|a, b| {
                a.top()
                    .total_cmp(&b.top())
                    .then(a.left().total_cmp(&b.left()))
            })
    }

    /// The command bar's strip: across the top of the page.
    fn bar_area(&self) -> Rect {
        let page = self.driver.runtime.shell.page_rect();
        Rect::from_min_size(page.min, vec2(page.width(), 44.0))
    }

    /// The names of the command bar's buttons, left to right.
    fn bar_names(&self) -> Vec<String> {
        let area = self.bar_area();
        let mut found: Vec<(f32, String)> = self
            .nodes
            .iter()
            .filter(|(_, rect, _)| area.contains(rect.center()) && rect.width() < 60.0)
            .map(|(label, rect, _)| (rect.left(), label.clone()))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.dedup_by(|a, b| a.1 == b.1);
        found.into_iter().map(|(_, name)| name).collect()
    }

    fn on(&self, name: &str, area: Rect) -> Option<bool> {
        self.nodes
            .iter()
            .find(|(label, rect, _)| label == name && area.contains(rect.center()))
            .and_then(|(_, _, on)| *on)
    }

    /// Whether the control `name` in `area` can be pressed now.
    fn enabled(&self, name: &str, area: Rect) -> bool {
        self.node_in(name, area).is_some()
            && !self
                .disabled
                .iter()
                .any(|(label, rect)| label == name && area.contains(rect.center()))
    }

    fn press(&mut self, at: Pos2, button: PointerButton) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: self.held,
        };
        let (down, up) = (press(true), press(false));
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![down]);
        self.step(vec![up]);
        // P4-8's dialogs take a few frames to find how tall they are.
        self.settle(8);
    }

    /// A click on the control `name` in `area`.
    fn click(&mut self, name: &str, area: Rect) {
        let at = self
            .node_in(name, area)
            .unwrap_or_else(|| panic!("{name:?} is drawn: {:?}", self.names()))
            .center();
        self.press(at, PointerButton::Primary);
    }

    /// A right click on the control `name` in `area`.
    fn right_click(&mut self, name: &str, area: Rect) {
        let at = self
            .node_in(name, area)
            .unwrap_or_else(|| panic!("{name:?} is drawn: {:?}", self.names()))
            .center();
        self.press(at, PointerButton::Secondary);
    }

    /// A click on the text `text`, where the last frame drew it.
    fn click_text(&mut self, text: &str) {
        let at = self
            .driver
            .rect_of(text)
            .unwrap_or_else(|| {
                let texts = &self.driver.texts;
                let last = &texts[texts.len().saturating_sub(60)..];
                panic!("{text:?} is on screen, the last drawn: {last:?}")
            })
            .center();
        self.press(at, PointerButton::Primary);
    }

    /// The pointer comes to rest on the text `text`, as a hand moves it
    /// there (a nested menu opens under a pointer that rests).
    fn hover_text(&mut self, text: &str) {
        let to = self
            .driver
            .rect_of(text)
            .unwrap_or_else(|| panic!("{text:?} is on screen"))
            .center();
        let from = self
            .driver
            .ctx
            .input(|input| input.pointer.latest_pos())
            .unwrap_or(to);
        for index in 1..=8 {
            let at = from + (to - from) * (index as f32 / 8.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.settle(8);
    }

    /// The dialog's Close button: the lowest control of that name (an
    /// entry of the File menu is called Close too).
    fn close_dialog(&mut self) {
        let at = self
            .nodes
            .iter()
            .filter(|(label, _, _)| label == "Close")
            .map(|(_, rect, _)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
            .expect("the dialog's Close")
            .center();
        self.press(at, PointerButton::Primary);
    }

    /// A drag from `from` to `to` with the primary button.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: self.held,
        };
        let (down, up) = (press(from, true), press(to, false));
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![down]);
        for index in 1..=20 {
            let at = from + (to - from) * (index as f32 / 20.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![up]);
        self.settle(6);
    }

    fn names(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect()
    }

    /// The text of a file in Sue Paint's data folder.
    fn file(&self, name: &str) -> String {
        std::fs::read_to_string(self.driver.root.join("data").join("paint").join(name))
            .unwrap_or_default()
    }

    /// The whole window.
    fn all(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))
    }

    /// Picks `category` in the settings dialog's first dropdown, finds
    /// its entry `entry` by the search and adds it with Add.
    fn add_from(&mut self, category: &str, entry: &str) {
        let search = self.find_entry(category, entry);
        self.click_text("Add");
        self.clear_search(search);
    }

    /// Picks the entry `entry` of `category` at the left without adding
    /// it.
    fn add_from_picked(&mut self, category: &str, entry: &str) {
        let search = self.find_entry(category, entry);
        self.clear_search(search);
    }

    /// Shows `category`, searches it for `entry` and picks that entry;
    /// where the search field is.
    fn find_entry(&mut self, category: &str, entry: &str) -> Rect {
        let all = self.all();
        self.click("Category", all);
        self.click_text(category);
        let search = self.driver.rect_of("Search").expect("the search field");
        self.press(search.center(), PointerButton::Primary);
        self.step(vec![egui::Event::Text(entry.to_owned())]);
        self.settle(3);
        let column = Rect::from_min_max(
            Pos2::new(search.left() - 40.0, search.bottom()),
            Pos2::new(search.left() + 240.0, 900.0),
        );
        let at = self
            .driver
            .texts
            .iter()
            .filter(|(text, rect)| text == entry && column.contains(rect.center()))
            .map(|(_, rect)| rect.center())
            .next()
            .unwrap_or_else(|| panic!("{entry:?} in {category}"));
        self.press(at, PointerButton::Primary);
        search
    }

    /// Empties the search field at `search`.
    fn clear_search(&mut self, search: Rect) {
        self.press(search.center(), PointerButton::Primary);
        self.step(vec![key(egui::Key::A, Modifiers::COMMAND)]);
        self.step(vec![key(egui::Key::Backspace, Modifiers::NONE)]);
        self.settle(2);
    }
}

/// Command Bar Settings puts a tool, a sub tool, a drawing color and a
/// palette that pops up on the bar, each with its icon (or swatch); each
/// does what it says, and the bar file keeps them (WS-7, WS-8, CT-31).
#[test]
fn command_bar_settings_put_tools_sub_tools_colors_and_palettes_on_the_bar() {
    let mut bar = Bar::new("paint-bar-settings");
    bar.driver.command("file.command_bar");
    bar.settle(12);
    assert!(
        bar.driver.rect_of("On the Command Bar").is_some(),
        "the dialog shows the bar's buttons"
    );
    bar.add_from("Tools", "Eraser");
    bar.add_from("Sub Tools", "G-pen");
    bar.add_from("Pop-up Palettes", "Color Slider");
    bar.add_from("Pop-up Palettes", "Color Wheel");
    bar.add_from("Drawing Colors", "Drawing Color #000000");
    bar.close_dialog();
    bar.settle(6);
    let kept = bar.file("command_bar.txt");
    for line in [
        "tool eraser",
        "sub-tool pen G-pen",
        "popup color_slider",
        "popup color\n",
        "color #000000",
    ] {
        assert!(kept.contains(line), "{line:?} in {kept}");
    }
    let names = bar.bar_names();
    for name in [
        "Eraser",
        "G-pen",
        "Color Slider",
        "Color Wheel",
        "Drawing Color #000000",
    ] {
        assert!(
            names.iter().any(|found| found == name),
            "{name} on {names:?}"
        );
    }
    // The tool's button says its key in its tooltip, as menus write it.
    let eraser = bar.node_in("Eraser", bar.bar_area()).unwrap().center();
    assert!(!bar.driver.texts.iter().any(|(text, _)| text == "E"));
    bar.step(vec![egui::Event::PointerMoved(eraser)]);
    bar.settle(40);
    assert!(
        bar.driver.texts.iter().any(|(text, _)| text == "E"),
        "the Eraser's tip shows E"
    );
    // The tool's button picks it and lights up.
    let area = bar.bar_area();
    assert_eq!(bar.on("Eraser", area), Some(false));
    bar.click("Eraser", area);
    assert_eq!(bar.on("Eraser", area), Some(true));
    // The sub tool's picks the pen's G-pen.
    bar.click("G-pen", area);
    assert_eq!(bar.on("G-pen", area), Some(true));
    assert_eq!(bar.on("Eraser", area), Some(false));
    // The Color Slider, behind the Color Wheel's tab, pops up under its
    // button; a click elsewhere puts it away.
    let before = bar.driver.texts.len();
    bar.click("Color Slider", area);
    assert!(bar.driver.texts.len() > before, "the palette pops up");
    bar.press(Pos2::new(700.0, 500.0), PointerButton::Primary);
    assert_eq!(
        bar.driver.texts.len(),
        before,
        "a click elsewhere puts it away"
    );
    // The Color Wheel is on screen already: its button draws no second
    // one.
    bar.click("Color Wheel", area);
    assert_eq!(bar.driver.texts.len(), before, "one Color Wheel on screen");
}

/// The right-click menu of a button: Delete takes it off, Add Drawing
/// Color and Add Separator put one after it, Change Order says how drags
/// move buttons, and Command Bar Settings opens the dialog (WS-4).
#[test]
fn the_right_click_menu_of_the_command_bar() {
    let mut bar = Bar::new("paint-bar-menu");
    let area = bar.bar_area();
    assert!(bar.bar_names().iter().any(|name| name == "Delete"));
    bar.right_click("Delete", area);
    for item in [
        "Icon Settings...",
        "Delete",
        "Add Drawing Color",
        "Insert Drawing Color into Group",
        "Add Separator",
        "Change Order",
        "Command Bar Settings...",
    ] {
        assert!(bar.driver.rect_of(item).is_some(), "{item} in the menu");
    }
    bar.click_text("Delete");
    assert!(!bar.bar_names().iter().any(|name| name == "Delete"));
    let kept = bar.file("command_bar.txt");
    assert!(kept.starts_with("sue-command-bar 1\n"), "kept: {kept}");
    assert!(kept.contains("command edit.undo\n"), "{kept}");
    assert!(!kept.contains("command edit.clear\n"), "{kept}");
    // A drawing color after Undo.
    bar.right_click("Undo", area);
    bar.click_text("Add Drawing Color");
    let names = bar.bar_names();
    let undo = names.iter().position(|name| name == "Undo").unwrap();
    assert!(names[undo + 1].starts_with("Drawing Color #"), "{names:?}");
    // Insert Drawing Color into Group makes the color a group.
    let color = names[undo + 1].clone();
    bar.right_click(&color, area);
    bar.click_text("Insert Drawing Color into Group");
    assert!(bar.file("command_bar.txt").contains("group\ncolor #"));
    // Change Order: Drag.
    bar.right_click("Redo", area);
    bar.hover_text("Change Order");
    bar.click_text("Drag");
    assert!(bar.file("command_bar.txt").contains("reorder drag\n"));
    // Command Bar Settings.
    bar.right_click("Redo", area);
    bar.click_text("Command Bar Settings...");
    assert!(bar.driver.rect_of("On the Command Bar").is_some());
}

/// Buttons move by a drag with Ctrl held, as Clip Studio Paint's Change
/// Order starts; a plain drag moves nothing until the order is Drag
/// (WS-3).
#[test]
fn command_bar_buttons_move_by_ctrl_drag() {
    let mut bar = Bar::new("paint-bar-drag");
    let area = bar.bar_area();
    let names = bar.bar_names();
    let undo = names.iter().position(|name| name == "Undo").unwrap();
    assert_eq!(names[undo + 1], "Redo");
    let from = bar.node_in("Redo", area).unwrap().center();
    let to = bar.node_in("Undo", area).unwrap().left_center() + vec2(2.0, 0.0);
    // A plain drag: nothing moves.
    bar.drag(from, to);
    let names = bar.bar_names();
    let undo = names.iter().position(|name| name == "Undo").unwrap();
    assert_eq!(names[undo + 1], "Redo", "a plain drag moves nothing");
    // With Ctrl, Redo goes before Undo.
    bar.held = Modifiers::COMMAND;
    bar.drag(from, to);
    bar.held = Modifiers::NONE;
    let names = bar.bar_names();
    let redo = names.iter().position(|name| name == "Redo").unwrap();
    assert_eq!(names[redo + 1], "Undo", "{names:?}");
    let kept = bar.file("command_bar.txt");
    assert!(
        kept.find("command edit.redo").unwrap() < kept.find("command edit.undo").unwrap(),
        "{kept}"
    );
    // Change Order, Drag: now a plain drag moves Undo back before Redo.
    bar.right_click("Redo", area);
    bar.hover_text("Change Order");
    bar.click_text("Drag");
    let from = bar.node_in("Undo", area).unwrap().center();
    let to = bar.node_in("Redo", area).unwrap().left_center() + vec2(2.0, 0.0);
    bar.drag(from, to);
    let names = bar.bar_names();
    let undo = names.iter().position(|name| name == "Undo").unwrap();
    assert_eq!(names[undo + 1], "Redo", "{names:?}");
    assert!(bar.file("command_bar.txt").contains("reorder drag\n"));
}

/// The timelapse button sits at the end of the command bar, as in Clip
/// Studio Paint, and lights while it records (FG-36).
#[test]
fn the_timelapse_button_is_on_the_command_bar() {
    let mut bar = Bar::new("paint-bar-timelapse");
    let area = bar.bar_area();
    let names = bar.bar_names();
    assert!(
        names.iter().any(|name| name == "Record Timelapse"),
        "{names:?}"
    );
    assert_eq!(bar.on("Record Timelapse", area), Some(false));
    bar.click("Record Timelapse", area);
    assert_eq!(bar.on("Record Timelapse", area), Some(true));
}

/// Window, Quick Access shows the palette with Set 1 and Set 2, Set 1
/// holding Clip Studio Paint's buttons with their names, the last of them
/// Quick Access Settings; its buttons run their commands; a new set can
/// be made and deleted; a button dragged onto the command bar is copied
/// there (WS-2, FG-38, FGV-55).
#[test]
fn quick_access_has_sets_of_named_buttons() {
    let mut bar = Bar::new("paint-quick-access");
    bar.driver.command("window.quick_access");
    bar.settle(6);
    for set in ["Set 1", "Set 2"] {
        assert!(bar.driver.rect_of(set).is_some(), "{set} tab");
    }
    for name in [
        "Undo",
        "Redo",
        "Delete",
        "Cut",
        "Copy",
        "Paste",
        "Flip Horizontal",
        "Quick Access Settings",
    ] {
        assert!(
            bar.driver.texts.iter().any(|(text, _)| text == name),
            "{name} in Quick Access"
        );
    }
    // Its own button opens its settings. Set 1 is taller than the
    // palette that pops out of the strip (P4-9), so the button sits past
    // the scrolled end: its command is what it runs.
    bar.driver.command("window.quick_access.settings");
    bar.settle(12);
    assert!(bar.driver.rect_of("In This Set").is_some());
    bar.close_dialog();
    bar.settle(4);
    // Opened beside the strip, it went away with the click outside it;
    // Window, Quick Access brings it back.
    if bar.driver.rect_of("Set 2").is_none() {
        bar.driver.command("window.quick_access");
        bar.settle(6);
    }
    // Set 2 shows the second set.
    bar.click_text("Set 2");
    assert!(bar.file("quick_access.txt").contains("shown 1\n"));
    // A new set, then gone again.
    bar.driver.command("window.quick_access.new_set");
    bar.settle(4);
    assert!(bar.driver.rect_of("Set 3").is_some());
    bar.driver.command("window.quick_access.delete_set");
    bar.settle(4);
    assert!(bar.driver.rect_of("Set 3").is_none());
    // A button dragged from Quick Access onto the command bar is copied.
    bar.click_text("Set 1");
    let cut = bar
        .driver
        .rect_of("Cut")
        .expect("Cut in Quick Access")
        .center();
    let area = bar.bar_area();
    assert!(!bar.bar_names().iter().any(|name| name == "Cut"));
    let to = bar
        .node_in("Fit to Screen", area)
        .expect("Fit to Screen on the bar")
        .right_center()
        + vec2(2.0, 0.0);
    bar.held = Modifiers::COMMAND;
    bar.drag(cut, to);
    bar.held = Modifiers::NONE;
    assert!(
        bar.bar_names().iter().any(|name| name == "Cut"),
        "{:?}",
        bar.bar_names()
    );
    assert!(bar.file("command_bar.txt").contains("command edit.cut"));
}

/// Icon Settings gives a button another icon by its name in Sue's set,
/// kept in the file so an icon pack replaces it too (WS-4, WS-7).
#[test]
fn icon_settings_give_a_button_another_icon() {
    let mut bar = Bar::new("paint-bar-icon");
    let area = bar.bar_area();
    bar.right_click("Undo", area);
    bar.click_text("Icon Settings...");
    let all = bar.all();
    assert!(bar.node_in("star", all).is_some(), "the icons show");
    bar.click("star", all);
    assert!(
        bar.file("command_bar.txt")
            .contains("command edit.undo\nicon star\n")
    );
    // Use Its Own Icon takes it back.
    bar.right_click("Undo", area);
    bar.click_text("Icon Settings...");
    bar.click_text("Use Its Own Icon");
    assert!(!bar.file("command_bar.txt").contains("icon star"));
}

/// Command Bar Settings puts an Auto Action on the bar, groups two
/// commands under one button with Insert into Group, and Restore Default
/// Layout puts Sue Paint's own buttons back (WS-7, WS-8).
#[test]
fn command_bar_settings_hold_auto_actions_and_groups_and_restore_the_default() {
    let mut bar = Bar::new("paint-bar-restore");
    bar.driver.command("file.command_bar");
    bar.settle(12);
    bar.add_from("Auto Action", "Rotate Canvas Right");
    assert!(
        bar.file("command_bar.txt")
            .contains("auto-action Rotate Canvas Right\n")
    );
    // Its button plays it: the canvas turns, a step that Undo takes back
    // while the canvas stays open (an Undo with nothing played would
    // undo the new canvas and close it).
    bar.close_dialog();
    let area = bar.bar_area();
    assert!(!bar.enabled("Redo", area), "nothing to redo yet");
    bar.click("Rotate Canvas Right", area);
    bar.click("Undo", area);
    assert!(bar.enabled("Redo", area), "the Auto Action made a step");
    assert!(bar.enabled("Fit to Screen", area), "the canvas is open");
    bar.driver.command("file.command_bar");
    bar.settle(12);
    // Undo, picked at the right, takes Redo into a group.
    let heading = bar
        .driver
        .rect_of("On the Command Bar")
        .expect("the bar's buttons");
    let right = Rect::from_min_max(
        Pos2::new(heading.left() - 20.0, heading.bottom()),
        Pos2::new(heading.left() + 260.0, 900.0),
    );
    let undo = bar
        .driver
        .texts
        .iter()
        .find(|(text, rect)| text == "Undo" && right.contains(rect.center()))
        .map(|(_, rect)| rect.center())
        .expect("Undo at the right");
    bar.press(undo, PointerButton::Primary);
    bar.add_from_picked("Main Menu", "Redo");
    bar.click_text("Insert into Group");
    assert!(
        bar.file("command_bar.txt")
            .contains("group\ncommand edit.undo\ncommand edit.redo\nend\n"),
        "{}",
        bar.file("command_bar.txt")
    );
    bar.click_text("Restore Default Layout");
    let kept = bar.file("command_bar.txt");
    assert!(!kept.contains("auto-action"), "{kept}");
    assert!(
        kept.contains("command edit.undo\ncommand edit.redo\n"),
        "{kept}"
    );
    bar.close_dialog();
    let names = bar.bar_names();
    assert!(!names.iter().any(|name| name == "Rotate Canvas Right"));
    assert!(names.iter().any(|name| name == "Undo"));
}
