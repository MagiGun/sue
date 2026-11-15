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

/// One frame with `events`, read through egui's accessibility tree:
/// every widget with a name, and where it is.
fn widgets(driver: &mut PaintDriver, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
    driver.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(1440.0, 900.0),
        )),
        events,
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += std::time::Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let mut found = Vec::new();
    if let Some(update) = &output.platform_output.accesskit_update {
        for (_, node) in &update.nodes {
            if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                found.push((
                    label.to_owned(),
                    egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                ));
            }
        }
    }
    output.drop_without_applying_deltas();
    found
}

/// Where the widget named `name` is, by egui's accessibility tree, after
/// a frame with nothing new; the topmost, then the leftmost.
fn widget(driver: &mut PaintDriver, name: &str) -> Option<egui::Rect> {
    widgets(driver, Vec::new())
        .into_iter()
        .filter(|(label, _)| label == name)
        .map(|(_, rect)| rect)
        .min_by(|a, b| {
            a.top()
                .total_cmp(&b.top())
                .then(a.left().total_cmp(&b.left()))
        })
}

/// Every widget named `name`.
fn all_widgets(driver: &mut PaintDriver, name: &str) -> Vec<egui::Rect> {
    widgets(driver, Vec::new())
        .into_iter()
        .filter(|(label, _)| label == name)
        .map(|(_, rect)| rect)
        .collect()
}

/// A click on the widget named `name`, found by its name and never by a
/// fixed position.
fn click_widget(driver: &mut PaintDriver, name: &str) {
    let rect = widget(driver, name).unwrap_or_else(|| panic!("{name:?} is on screen"));
    click_at_point(driver, rect.center(), egui::PointerButton::Primary);
}

/// A click of `button` at `at`, then a few frames.
fn click_at_point(driver: &mut PaintDriver, at: Pos2, button: egui::PointerButton) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![press(true)]);
    driver.step(vec![press(false)]);
    driver.settle(6);
}

/// Enter, as a person accepts a dialog.
fn enter(driver: &mut PaintDriver) {
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(8);
}

/// Selects what a text field holds and types `text` over it.
fn type_over(driver: &mut PaintDriver, text: &str) {
    driver.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    driver.step(vec![egui::Event::Text(text.to_owned())]);
    driver.settle(2);
}

/// The kept tool palette, as Sue Paint wrote it to its data folder: the
/// names of its `tool` lines in order.
fn kept_tools(driver: &PaintDriver) -> Vec<String> {
    let file = driver
        .root
        .join("data")
        .join("paint")
        .join("tools.suetools");
    let text = std::fs::read_to_string(file).unwrap_or_default();
    text.lines()
        .filter_map(|line| line.strip_prefix("tool\t"))
        .map(|line| line.split('\t').next().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_tool_is_dragged_to_another_place_and_the_order_is_kept() {
    let mut driver = PaintDriver::new("paint-tools-reorder");
    driver.settle(4);
    let pen = widget(&mut driver, "Pen (P)").expect("the Pen tool");
    let eraser = widget(&mut driver, "Eraser (E)").expect("the Eraser tool");
    assert!(pen.top() < eraser.top());
    // Let go on the bottom quarter of the Eraser: after it.
    let to = egui::pos2(eraser.center().x, eraser.bottom() - eraser.height() / 8.0);
    driver.drag(pen.center(), to, egui::PointerButton::Primary);
    let pen = widget(&mut driver, "Pen (P)").expect("the Pen tool");
    let eraser = widget(&mut driver, "Eraser (E)").expect("the Eraser tool");
    assert!(pen.top() > eraser.top(), "the Pen is under the Eraser now");
    driver.settle(2);
    let kept = kept_tools(&driver);
    let place = |name: &str| {
        kept.iter()
            .position(|found| found == name)
            .unwrap_or_else(|| panic!("{name} in {kept:?}"))
    };
    assert_eq!(place("Pen"), place("Eraser") + 1, "{kept:?}");
}

#[test]
fn a_sub_tool_dragged_onto_the_tool_palette_is_a_tool_and_goes_back() {
    let mut driver = PaintDriver::new("paint-tools-promote");
    driver.settle(4);
    let turnip = widget(&mut driver, "Turnip pen").expect("Turnip pen in the list");
    let eraser = widget(&mut driver, "Eraser (E)").expect("the Eraser tool");
    // Let go on the top quarter of the Eraser: a tool of its own before it.
    let to = egui::pos2(eraser.center().x, eraser.top() + eraser.height() / 8.0);
    driver.drag(turnip.center(), to, egui::PointerButton::Primary);
    // Its button on the tool palette (the Sub Tool palette shows its one
    // group's tab by the same name).
    let made = all_widgets(&mut driver, "Turnip pen")
        .into_iter()
        .find(|rect| rect.left() < 46.0)
        .expect("a Turnip pen tool on the tool palette");
    let eraser = widget(&mut driver, "Eraser (E)").expect("the Eraser tool");
    assert!(made.bottom() <= eraser.top() + 1.0);
    driver.settle(2);
    let kept = kept_tools(&driver);
    assert!(kept.iter().any(|name| name == "Turnip pen"), "{kept:?}");
    // Back: the Pen out, the new tool dragged onto its list.
    click_widget(&mut driver, "Pen (P)");
    let list = widget(&mut driver, "G-pen").expect("the Pen's list");
    driver.drag(made.center(), list.center(), egui::PointerButton::Primary);
    let rows = all_widgets(&mut driver, "Turnip pen");
    assert_eq!(rows.len(), 1, "one Turnip pen: {rows:?}");
    assert!(rows[0].left() > 46.0, "in the Sub Tool list again");
    driver.settle(2);
    assert!(!kept_tools(&driver).iter().any(|name| name == "Turnip pen"));
}

#[test]
fn the_sub_tool_list_has_its_buttons_menu_and_add_row() {
    let mut driver = PaintDriver::new("paint-tools-sub-buttons");
    driver.settle(4);
    // Duplicate opens a dialog with the name to give the copy.
    click_widget(&mut driver, "Duplicate Sub Tool");
    assert!(driver.rect_of("G-pen Copy").is_some(), "{:?}", driver.texts);
    type_over(&mut driver, "Inking pen");
    enter(&mut driver);
    assert!(
        widget(&mut driver, "Inking pen").is_some(),
        "the copy is listed"
    );
    // Delete takes the picked sub tool, the copy, away.
    click_widget(&mut driver, "Delete Sub Tool");
    assert!(widget(&mut driver, "Inking pen").is_none());
    assert!(widget(&mut driver, "G-pen").is_some());
    // The right-click menu: a new group from a sub tool on.
    let mapping = widget(&mut driver, "Mapping pen").expect("Mapping pen");
    click_at_point(
        &mut driver,
        mapping.center(),
        egui::PointerButton::Secondary,
    );
    for item in [
        "Duplicate Sub Tool...",
        "Rename Sub Tool...",
        "Delete Sub Tool",
        "New Group from Here...",
        "Make into a Tool",
        "Sub Tool Detail...",
    ] {
        assert!(
            driver.rect_of(item).is_some(),
            "{item} in {:?}",
            driver.texts
        );
    }
    driver.click_text("New Group from Here...");
    assert!(driver.rect_of("New Sub Tool Group").is_some());
    type_over(&mut driver, "Inks");
    enter(&mut driver);
    assert!(widget(&mut driver, "Inks").is_some(), "the new group's tab");
    assert!(
        widget(&mut driver, "Mapping pen").is_some(),
        "its first sub tool"
    );
    assert!(
        widget(&mut driver, "G-pen").is_none(),
        "the old group is not shown"
    );
    // Every list ends with a row that adds sub tools.
    driver.command("tool.fill");
    driver.settle(4);
    let add = widget(&mut driver, "Add Sub Tool").expect("Add Sub Tool");
    let last = widget(&mut driver, "Paint unpainted area").expect("the last sub tool");
    assert!(add.top() >= last.bottom() - 1.0);
}

/// A GIMP brush (`.gbr`, version 2) named `name`: a small round tip.
fn gimp_brush(name: &str) -> Vec<u8> {
    let side = 9_u32;
    let header = 28 + name.len() as u32 + 1;
    let mut bytes = Vec::new();
    for value in [header, 2, side, side, 1] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(b"GIMP");
    bytes.extend_from_slice(&25_u32.to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(0);
    for y in 0..side {
        for x in 0..side {
            let (dx, dy) = (x as f32 - 4.0, y as f32 - 4.0);
            bytes.push(if dx * dx + dy * dy <= 16.0 { 255 } else { 0 });
        }
    }
    bytes
}

#[test]
fn a_brush_file_let_go_on_sue_paint_lands_in_the_group_shown_and_is_picked() {
    let mut driver = PaintDriver::fresh("paint-tools-brush-drop");
    driver.settle(4);
    let file = driver.root.join("Round dot.gbr");
    std::fs::create_dir_all(&driver.root).unwrap();
    std::fs::write(&file, gimp_brush("Round dot")).unwrap();
    // A file let go on the window opens where it belongs.
    driver
        .runtime
        .request(ShellRequest::OpenFile(file), &mut NoTextures, driver.now);
    driver.settle(8);
    let row = widget(&mut driver, "Round dot").expect("the brush is a sub tool");
    let last = widget(&mut driver, "Calligraphy").expect("the Pen group's last");
    assert!(
        row.top() >= last.bottom() - 1.0,
        "at the end of the Pen group"
    );
    // Picked: Tool Property shows its name too.
    let shown = driver
        .texts
        .iter()
        .filter(|(text, _)| text == "Round dot")
        .count();
    assert!(
        shown >= 2,
        "listed and in Tool Property: {:?}",
        driver.texts
    );
}

/// The texts the last frame drew inside `area`.
fn texts_inside(driver: &PaintDriver, area: egui::Rect) -> Vec<String> {
    driver
        .texts
        .iter()
        .filter(|(_, rect)| area.contains(rect.center()))
        .map(|(text, _)| text.clone())
        .collect()
}

#[test]
fn sub_tool_detail_shows_every_setting_and_its_eye_hides_it_in_tool_property() {
    let mut driver = PaintDriver::fresh("paint-tools-detail");
    driver.settle(4);
    // Tool Property, in the left column, shows the G-pen's Opacity.
    let property = egui::Rect::from_min_max(egui::pos2(46.0, 280.0), egui::pos2(355.0, 900.0));
    assert!(
        texts_inside(&driver, property)
            .iter()
            .any(|text| text == "Opacity"),
        "{:?}",
        driver.texts
    );
    // Its wrench opens Sub Tool Detail.
    click_widget(
        &mut driver,
        "Sub Tool Detail: every setting, and which ones Tool Property shows",
    );
    driver.settle(4);
    let detail = all_widgets(&mut driver, "Sub Tool Detail")
        .into_iter()
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("Sub Tool Detail's window");
    for category in ["Brush Size", "Ink", "Anti-aliasing", "Correction"] {
        assert!(
            all_widgets(&mut driver, category)
                .iter()
                .any(|rect| detail.contains(rect.center())),
            "the category {category} is listed"
        );
    }
    assert!(driver.rect_of("Show Categories in Tool Property").is_some());
    // Ink: its rows, each with an eye, and its help.
    let ink = all_widgets(&mut driver, "Ink")
        .into_iter()
        .find(|rect| detail.contains(rect.center()))
        .expect("Ink");
    click_at_point(&mut driver, ink.center(), egui::PointerButton::Primary);
    assert!(
        driver
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("How strongly the brush lays its color")),
        "Ink's help: {:?}",
        driver.texts
    );
    let opacity = driver
        .texts
        .iter()
        .filter(|(text, rect)| text == "Opacity" && detail.contains(rect.center()))
        .map(|(_, rect)| *rect)
        .next()
        .expect("Opacity in Sub Tool Detail");
    let eyes = all_widgets(&mut driver, "Hide in Tool Property");
    let eye = eyes
        .iter()
        .filter(|rect| rect.right() <= opacity.left() + 1.0)
        .min_by(|a, b| {
            (a.center().y - opacity.center().y)
                .abs()
                .total_cmp(&(b.center().y - opacity.center().y).abs())
        })
        .copied()
        .expect("Opacity's eye");
    click_at_point(&mut driver, eye.center(), egui::PointerButton::Primary);
    assert!(
        !texts_inside(&driver, property)
            .iter()
            .any(|text| text == "Opacity"),
        "Tool Property leaves Opacity out: {:?}",
        texts_inside(&driver, property)
    );
    assert!(
        widget(&mut driver, "Show in Tool Property").is_some(),
        "the eye is shut"
    );
    // Show Categories names each category over its rows in Tool Property.
    click_widget(&mut driver, "Show Categories in Tool Property");
    assert!(
        texts_inside(&driver, property)
            .iter()
            .any(|text| text == "Brush Size")
            && texts_inside(&driver, property)
                .iter()
                .any(|text| text == "Correction"),
        "{:?}",
        texts_inside(&driver, property)
    );
}

#[test]
fn a_locked_sub_tool_keeps_its_settings() {
    let mut driver = PaintDriver::fresh("paint-tools-lock");
    driver.settle(4);
    click_widget(&mut driver, "Lock the settings of this sub tool");
    assert!(
        driver
            .rect_of("Settings locked: unlock them to make changes")
            .is_some(),
        "{:?}",
        driver.texts
    );
    assert!(widget(&mut driver, "Unlock the settings of this sub tool").is_some());
    // A drag along Opacity's slider changes nothing while locked.
    let slider = |driver: &mut PaintDriver| {
        all_widgets(driver, "Opacity")
            .into_iter()
            .find(|rect| rect.left() < 400.0 && rect.width() > 100.0)
            .expect("Opacity's slider")
    };
    let row = slider(&mut driver);
    let from = egui::pos2(row.right() - 4.0, row.center().y);
    let to = egui::pos2(row.left() + row.width() * 0.5, row.center().y);
    let value = |driver: &PaintDriver| {
        driver
            .texts
            .iter()
            .filter(|(_, rect)| (rect.center().y - row.center().y).abs() < row.height())
            .filter(|(text, rect)| rect.left() < 400.0 && text.parse::<f32>().is_ok())
            .map(|(text, _)| text.clone())
            .next()
    };
    let before = value(&driver);
    driver.drag(from, to, egui::PointerButton::Primary);
    assert_eq!(value(&driver), before, "locked");
    click_widget(&mut driver, "Unlock the settings of this sub tool");
    driver.drag(from, to, egui::PointerButton::Primary);
    assert_ne!(value(&driver), before, "unlocked, the drag changes it");
}

#[test]
fn the_tool_palette_menu_deletes_brings_back_renames_and_hides_colors() {
    let mut driver = PaintDriver::new("paint-tools-menu");
    driver.settle(4);
    click_widget(&mut driver, "Tool: palette menu");
    for item in [
        "Tool Settings...",
        "Delete Tool",
        "Add Tool from Default",
        "Reset Tool Palette",
        "Import Tool Layout...",
        "Export Tool Layout...",
        "Show Color Icon",
    ] {
        assert!(
            driver.rect_of(item).is_some(),
            "{item} in {:?}",
            driver.texts
        );
    }
    // Delete Tool takes the Pen, the tool out, off the palette.
    let pen = widget(&mut driver, "Pen (P)").expect("the Pen");
    driver.click_text("Delete Tool");
    assert!(widget(&mut driver, "Pen (P)").is_none(), "the Pen is gone");
    driver.command("subtool.pen.Turnip_pen");
    driver.settle(4);
    assert!(
        driver
            .rect_of("That sub tool is not in the tool palette any more")
            .is_some(),
        "{:?}",
        driver.texts
    );
    // Add Tool from Default brings it back where it was.
    click_widget(&mut driver, "Tool: palette menu");
    driver.click_text("Add Tool from Default");
    let back = widget(&mut driver, "Pen (P)").expect("the Pen is back");
    assert!((back.top() - pen.top()).abs() < 1.0, "{back:?} {pen:?}");
    // Tool Settings names the tool out.
    click_widget(&mut driver, "Pen (P)");
    click_widget(&mut driver, "Tool: palette menu");
    driver.click_text("Tool Settings...");
    assert!(
        driver.rect_of("Tool Settings").is_some(),
        "{:?}",
        driver.texts
    );
    type_over(&mut driver, "Inks");
    enter(&mut driver);
    assert!(
        widget(&mut driver, "Inks (P)").is_some(),
        "renamed, its key kept"
    );
    // Show Color Icon off: no color circles under the tools.
    let colors_on_strip = |driver: &mut PaintDriver| {
        all_widgets(driver, "Main color")
            .iter()
            .any(|rect| rect.left() < 46.0)
    };
    assert!(colors_on_strip(&mut driver));
    click_widget(&mut driver, "Tool: palette menu");
    driver.click_text("Show Color Icon");
    assert!(!colors_on_strip(&mut driver), "the colors are hidden");
}

#[test]
fn fill_and_auto_select_show_their_area_and_reference_rows() {
    let mut driver = PaintDriver::fresh("paint-tools-fill-rows");
    driver.settle(4);
    driver.command("tool.fill");
    driver.settle(4);
    driver.click_text("Refer Other Layers");
    for row in [
        "Apply to connected pixels only",
        "Close gap",
        "Tolerance",
        "Area scaling",
        "Refer multiple",
        "Leave out draft layers",
        "Leave out text layers",
        "Leave out paper layers",
        "Leave out locked layers",
        "Anti-aliasing",
    ] {
        assert!(
            driver.rect_of(row).is_some() || widget(&mut driver, row).is_some(),
            "Fill shows {row}: {:?}",
            driver.texts
        );
    }
    driver.command("tool.auto_select");
    driver.settle(4);
    for row in [
        "Close gap",
        "Area scaling",
        "Apply to connected pixels only",
    ] {
        assert!(
            driver.rect_of(row).is_some() || widget(&mut driver, row).is_some(),
            "Auto Select shows {row}: {:?}",
            driver.texts
        );
    }
}
