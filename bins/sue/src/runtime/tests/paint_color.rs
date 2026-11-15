//! Sue Paint: the color palettes, Color Settings and the color sets
//! (P4-4). Controls are found by what they tell assistive technology
//! (their names) or by their widget ids, never by fixed places.

use super::*;
use egui::{Event, Modifiers, PointerButton, Rect};

/// Runs one frame and returns every control that named itself to
/// assistive technology, with its rect.
fn look(paint: &mut PaintDriver) -> Vec<(String, Rect)> {
    paint.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    paint.texts = text_rects(&output);
    let named = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let label = node.label()?.to_owned();
            let bounds = node.bounds()?;
            Some((
                label,
                Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect();
    output.drop_without_applying_deltas();
    named
}

/// The rect of the last control named `name` (the topmost layer draws
/// last), after a frame to look.
fn control(paint: &mut PaintDriver, name: &str) -> Rect {
    let named = look(paint);
    named
        .iter()
        .rev()
        .find(|(label, _)| label == name)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| {
            let names: Vec<&str> = named.iter().map(|(label, _)| label.as_str()).collect();
            panic!("nothing is named {name:?}: {names:?}")
        })
}

/// Where the widget `id` was drawn last.
fn widget(paint: &PaintDriver, id: egui::Id) -> Rect {
    paint
        .ctx
        .read_response(id)
        .map(|response| response.rect)
        .unwrap_or_else(|| panic!("the widget {id:?} is drawn"))
}

fn press(at: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// A click of `button` at `at`; what the frames after draw.
fn click(paint: &mut PaintDriver, at: Pos2, button: PointerButton) {
    paint.step(vec![Event::PointerMoved(at)]);
    paint.step(vec![press(at, button, true)]);
    paint.step(vec![press(at, button, false)]);
    paint.settle(4);
}

/// Two quick clicks at `at`.
fn double_click(paint: &mut PaintDriver, at: Pos2) {
    paint.step(vec![Event::PointerMoved(at)]);
    for _ in 0..2 {
        paint.step(vec![press(at, PointerButton::Primary, true)]);
        paint.step(vec![press(at, PointerButton::Primary, false)]);
    }
    paint.settle(4);
}

/// Replaces what the focused field holds with `text`.
fn type_over(paint: &mut PaintDriver, text: &str) {
    paint.step(vec![key(egui::Key::A, Modifiers::COMMAND)]);
    paint.step(vec![Event::Text(text.to_owned())]);
    paint.settle(2);
}

/// The color set file in the test's data folder.
fn sets_file(paint: &PaintDriver) -> String {
    std::fs::read_to_string(paint.root.join("data").join("paint").join("color_sets.txt"))
        .unwrap_or_default()
}

/// The swatch `cell` of the set showing (set `set`).
fn swatch(set: usize, cell: usize) -> egui::Id {
    egui::Id::unique(("sue-paint-swatch", "set", set, cell))
}

/// The Color Set palette's main (`false`) or sub color.
fn drawing_chip(sub: bool) -> egui::Id {
    egui::Id::unique(("sue-paint-set-drawing-color", sub))
}

/// Gaps WS-11, CT-28 and A25-26 (the dialog half): a double click on the
/// main color opens Color Settings; a hex code typed there and OK make
/// it the main color, which the dialog shows when opened again.
#[test]
fn a_double_click_on_the_main_color_opens_color_settings() {
    let mut paint = PaintDriver::new("paint-color-settings");
    paint.settle(4);
    look(&mut paint);
    let main = widget(&paint, drawing_chip(false));
    double_click(&mut paint, main.center());
    look(&mut paint);
    assert!(
        paint.rect_of("Color Settings").is_some(),
        "the dialog is open: {:?}",
        paint.texts
    );
    // Its numbers in four models, and the hex code.
    for name in [
        "H", "S", "V", "R", "G", "B", "C", "M", "Y", "K", "L", "a", "b",
    ] {
        control(&mut paint, name);
    }
    let hex = control(&mut paint, "Hex");
    click(&mut paint, hex.center(), PointerButton::Primary);
    type_over(&mut paint, "#336699");
    paint.click_text("OK");
    assert!(paint.rect_of("Color Settings").is_none(), "OK closes it");
    // Opened again on the command, it shows the color it set.
    paint.command("color.settings");
    paint.settle(4);
    assert!(
        paint.texts.iter().any(|(text, _)| text == "#336699"),
        "the main color is #336699: {:?}",
        paint.texts
    );
    paint.click_text("Cancel");
    assert!(paint.rect_of("Color Settings").is_none());
}

/// Gaps WS-23 and FG-33 (the Color Slider half): the Color Slider palette
/// switches between HSV, RGB and CMYK.
#[test]
fn the_color_slider_switches_between_hsv_rgb_and_cmyk() {
    let mut paint = PaintDriver::new("paint-color-slider");
    // In this layout Color Slider is a tab beside the Color Wheel.
    paint.click_text("Color Slider");
    let hsv = control(&mut paint, "HSV");
    let cmyk = control(&mut paint, "CMYK");
    assert!(hsv.top() == cmyk.top() && hsv.left() < cmyk.left());
    control(&mut paint, "H");
    click(&mut paint, cmyk.center(), PointerButton::Primary);
    let named = look(&mut paint);
    for name in ["C", "M", "Y", "K"] {
        assert!(
            named.iter().any(|(label, _)| label == name),
            "{name} in CMYK"
        );
    }
    let rgb = control(&mut paint, "RGB");
    click(&mut paint, rgb.center(), PointerButton::Primary);
    let named = look(&mut paint);
    assert!(named.iter().any(|(label, _)| label == "R"));
    assert!(!named.iter().any(|(label, _)| label == "K"), "no K in RGB");
}

/// Gaps A25-26 and A25V-21: a new set is asked for its name and starts
/// as empty cells; a click on one puts the drawing color there; a color
/// takes a name; the set is renamed; all of it is kept in the file.
#[test]
fn a_new_color_set_is_named_filled_and_kept() {
    let mut paint = PaintDriver::new("paint-color-set-new");
    paint.settle(4);
    let wrench = control(&mut paint, "Color set settings");
    click(&mut paint, wrench.center(), PointerButton::Primary);
    paint.click_text("New Color Set...");
    assert!(
        paint.rect_of("New Color Set").is_some(),
        "{:?}",
        paint.texts
    );
    let name = control(&mut paint, "Name");
    click(&mut paint, name.center(), PointerButton::Primary);
    type_over(&mut paint, "Inks");
    paint.click_text("OK");
    look(&mut paint);
    assert!(
        paint.rect_of("Inks").is_some(),
        "the set shows: {:?}",
        paint.texts
    );
    // Four built-in sets, then the new one: set 4. Its cells are empty.
    let first = widget(&paint, swatch(4, 0));
    click(&mut paint, first.center(), PointerButton::Primary);
    let file = sets_file(&paint);
    let line = file
        .lines()
        .find(|line| line.starts_with("Inks\t"))
        .unwrap_or_else(|| panic!("the set is kept: {file}"));
    assert!(line.starts_with("Inks\t#000000 - -"), "{line}");
    // A right click: Change Color Name.
    let first = widget(&paint, swatch(4, 0));
    click(&mut paint, first.center(), PointerButton::Secondary);
    paint.click_text("Change Color Name...");
    let name = control(&mut paint, "Name");
    click(&mut paint, name.center(), PointerButton::Primary);
    type_over(&mut paint, "Line art");
    paint.click_text("OK");
    assert!(
        sets_file(&paint).contains("Inks\t#000000/Line%20art -"),
        "{}",
        sets_file(&paint)
    );
    // Rename the set.
    let wrench = control(&mut paint, "Color set settings");
    click(&mut paint, wrench.center(), PointerButton::Primary);
    paint.click_text("Rename Color Set...");
    let name = control(&mut paint, "Name");
    click(&mut paint, name.center(), PointerButton::Primary);
    type_over(&mut paint, "Lines");
    paint.step(vec![key(egui::Key::Enter, Modifiers::NONE)]);
    paint.settle(4);
    let file = sets_file(&paint);
    assert!(file.contains("Lines\t#000000/Line%20art"), "{file}");
    assert!(!file.contains("Inks\t"), "{file}");
    // Delete Color leaves the cell where it was.
    let first = widget(&paint, swatch(4, 0));
    click(&mut paint, first.center(), PointerButton::Secondary);
    paint.click_text("Delete Color");
    assert!(
        sets_file(&paint).contains("Lines\t- - -"),
        "{}",
        sets_file(&paint)
    );
}

/// Gap A25V-21: Edit Color Sets adds a standard set again and moves a
/// set by dragging its grip, and Alt and Up move the set picked.
#[test]
fn edit_color_sets_adds_a_standard_set_and_reorders() {
    let mut paint = PaintDriver::new("paint-color-set-edit");
    paint.settle(4);
    let wrench = control(&mut paint, "Color set settings");
    click(&mut paint, wrench.center(), PointerButton::Primary);
    paint.click_text("Edit Color Sets...");
    assert!(
        paint.rect_of("Edit Color Sets").is_some(),
        "{:?}",
        paint.texts
    );
    paint.click_text("Add Standard Set");
    // The popover's Grays, drawn over the list (which has a Grays too).
    let grays = control(&mut paint, "Grays");
    click(&mut paint, grays.center(), PointerButton::Primary);
    look(&mut paint);
    let added = control(&mut paint, "Grays 2");
    let top = control(&mut paint, "Default");
    // Its grip is at the start of its row.
    let grip = Pos2::new(added.left() + 8.0, added.center().y);
    paint.drag(
        grip,
        Pos2::new(grip.x, top.top() + 2.0),
        PointerButton::Primary,
    );
    paint.click_text("Close");
    let file = sets_file(&paint);
    let names: Vec<&str> = file
        .lines()
        .skip(1)
        .filter_map(|line| line.split('\t').next())
        .collect();
    assert_eq!(
        names,
        ["Grays 2", "Default", "Grays", "Skin Tones", "Pastels"],
        "{file}"
    );
    // Again, and Alt and Down moves the picked set (the first) down.
    let wrench = control(&mut paint, "Color set settings");
    click(&mut paint, wrench.center(), PointerButton::Primary);
    paint.click_text("Edit Color Sets...");
    let first = control(&mut paint, "Grays 2");
    click(&mut paint, first.center(), PointerButton::Primary);
    paint.step(vec![key(egui::Key::ArrowDown, Modifiers::ALT)]);
    paint.settle(2);
    paint.click_text("Close");
    let file = sets_file(&paint);
    assert!(
        file.lines()
            .nth(1)
            .is_some_and(|line| line.starts_with("Default\t"))
            && file
                .lines()
                .nth(2)
                .is_some_and(|line| line.starts_with("Grays 2\t")),
        "{file}"
    );
}

/// A Clip Studio Paint color set laid out as its files are (see
/// `colorsets::import_cls`), made here.
fn cls_file(name: &str, cells: &[Option<[u8; 3]>]) -> Vec<u8> {
    let part = |bytes: &[u8]| {
        let mut out = u32::try_from(bytes.len()).unwrap().to_le_bytes().to_vec();
        out.extend_from_slice(bytes);
        out
    };
    let mut head = vec![1, 0, b'x', 0, 0, 0, 0];
    head.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
    head.extend_from_slice(name.as_bytes());
    let mut body = Vec::new();
    for cell in cells {
        let mut one = match cell {
            Some([r, g, b]) => vec![*r, *g, *b, 255],
            None => vec![0; 4],
        };
        one.extend_from_slice(&[0; 4]);
        body.extend(part(&one));
    }
    let mut file = b"SLCC\x00\x01".to_vec();
    file.extend(part(&head));
    file.extend(part(&u32::try_from(cells.len()).unwrap().to_le_bytes()));
    file.extend(part(&body));
    file
}

/// Gap FG-33 (the Color Set half): a Clip Studio Paint `.cls` color set
/// imports, named as the file names it and with its empty cell; the
/// export is a GIMP palette that keeps that cell.
#[test]
fn clip_studio_color_sets_import_and_export_as_gimp_palettes() {
    let mut paint = PaintDriver::new("paint-color-set-cls");
    paint.settle(4);
    let file = paint.root.join("warm.cls");
    std::fs::write(
        &file,
        cls_file(
            "Warm colors",
            &[Some([255, 50, 100]), None, Some([1, 2, 3])],
        ),
    )
    .unwrap();
    let import = control(
        &mut paint,
        "Import a color set (GIMP .gpl, Clip Studio Paint .cls or a list of hex codes)",
    );
    click(&mut paint, import.center(), PointerButton::Primary);
    assert!(
        paint.runtime.answer_pick(&file, &mut NoTextures, paint.now),
        "the import asks for a file"
    );
    paint.settle(6);
    look(&mut paint);
    assert!(paint.rect_of("Warm colors").is_some(), "{:?}", paint.texts);
    assert!(
        sets_file(&paint).contains("Warm colors\t#ff3264 - #010203"),
        "{}",
        sets_file(&paint)
    );
    let out = paint.root.join("warm.gpl");
    let export = control(&mut paint, "Export this color set (GIMP .gpl)");
    click(&mut paint, export.center(), PointerButton::Primary);
    assert!(paint.runtime.answer_pick(&out, &mut NoTextures, paint.now));
    paint.settle(4);
    let gpl = std::fs::read_to_string(&out).expect("the palette is written");
    assert!(
        gpl.starts_with("GIMP Palette\nName: Warm colors\n"),
        "{gpl}"
    );
    assert!(gpl.contains("# Sue: empty cell"), "{gpl}");
}

/// Gap CT-27: the wheel's button switches between the HSV square and the
/// HLS triangle, every wheel at once: Color Settings' and the palette's.
#[test]
fn the_color_wheel_switches_between_square_and_triangle() {
    let mut paint = PaintDriver::new("paint-color-wheel-shape");
    paint.settle(4);
    paint.command("color.settings");
    paint.settle(4);
    let button = control(&mut paint, "Show the HLS triangle");
    click(&mut paint, button.center(), PointerButton::Primary);
    let named = look(&mut paint);
    assert!(
        named
            .iter()
            .any(|(label, _)| label == "Show the HSV square")
    );
    assert!(
        !named
            .iter()
            .any(|(label, _)| label == "Show the HLS triangle")
    );
    paint.click_text("Cancel");
    assert_eq!(
        sue_wm::wheel_shape(&paint.ctx),
        sue_wm::WheelShape::Triangle,
        "the palette's wheel draws the triangle too"
    );
}

/// Gap WS-20: Material is a palette of its own, never a tab of Color
/// Set, and Color Set and Color Wheel are palettes of their own.
#[test]
fn material_is_no_tab_of_the_color_set() {
    let mut paint = PaintDriver::new("paint-color-set-tabs");
    paint.settle(4);
    let set = paint.rect_of("Color Set").expect("the Color Set palette");
    let row = Rect::from_min_max(
        Pos2::new(set.left() - 40.0, set.top() - 4.0),
        Pos2::new(set.left() + 400.0, set.bottom() + 4.0),
    );
    assert!(
        text_in(&paint.texts, "Material", row).is_none(),
        "{:?}",
        paint.texts
    );
    // Hiding the Color Wheel leaves the Color Set where it was.
    paint.command("window.color_wheel");
    paint.settle(6);
    assert!(paint.rect_of("Color Set").is_some());
    look(&mut paint);
    assert!(
        paint.ctx.read_response(drawing_chip(true)).is_some(),
        "the Color Set palette still draws"
    );
}
