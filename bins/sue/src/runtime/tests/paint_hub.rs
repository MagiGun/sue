//! Sue Paint without its launcher (owner note 11, D8): with no canvas it
//! shows its menus, palettes and an empty canvas area with New and Open;
//! File, Back to the Hub and a phone's back button go to the Hub; its
//! materials are the Hub's Materials place; and what the launcher's
//! popups held lives in the Help menu, Settings and the places' menus.

use super::*;

/// Sue Paint's applet id.
fn paint_id() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// Sends Sue Paint to `target` (a route of its `navigate`).
fn go(paint: &mut PaintDriver, target: &str) -> bool {
    paint
        .runtime
        .applets
        .get_mut(&paint_id())
        .is_some_and(|applet| applet.navigate(target))
}

/// The Hub's Materials place, shown.
fn to_materials(paint: &mut PaintDriver) {
    paint.runtime.request(
        ShellRequest::HubPlace("materials".to_owned()),
        &mut NoTextures,
        paint.now,
    );
    paint.settle(80);
    assert!(paint.runtime.shell.hub_open(), "{:?}", paint.texts);
    assert_eq!(paint.runtime.shell.hub_place(), Some("materials"));
}

/// The pictures in the arena.
fn pictures(paint: &PaintDriver) -> usize {
    paint
        .runtime
        .engine
        .world
        .arena
        .iter()
        .filter(|(_, entity)| sue_model::picture::is_picture(entity))
        .count()
}

/// The names of the entities in the arena.
fn layer_names(paint: &PaintDriver) -> Vec<String> {
    paint
        .runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.name().to_owned())
        .collect()
}

/// Every entity in the arena, ids and content, in id order: what undo
/// and redo must give back exactly (rule 7).
fn entities(paint: &PaintDriver) -> Vec<sue_core::Entity> {
    let mut all: Vec<sue_core::Entity> = paint
        .runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.clone())
        .collect();
    all.sort_by_key(sue_core::Entity::id);
    all
}

/// The text of Sue's notice, if one shows.
fn notice(paint: &PaintDriver) -> String {
    paint
        .runtime
        .shell
        .message()
        .map(|(text, _)| text.to_owned())
        .unwrap_or_default()
}

/// A right click in the middle of `text`; what the frames after draw.
fn right_click_text(paint: &mut PaintDriver, text: &str) {
    let at = paint
        .rect_of(text)
        .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", paint.texts))
        .center();
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(6);
}

/// Tutorial step B1: closing the last canvas leaves Sue Paint's
/// workspace: the menu bar, the palettes (the Window menu still opens
/// more) and, where the canvas was, New and Open.
#[test]
fn closing_the_last_canvas_leaves_the_palettes_menus_and_new_and_open() {
    let mut paint = PaintDriver::new("paint-no-canvas");
    assert_eq!(pictures(&paint), 1);
    paint.command("file.close");
    paint.settle(20);
    assert_eq!(
        paint.runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID),
        "Sue Paint stays"
    );
    for text in [
        "No canvas is open",
        "New...",
        "Open...",
        "File",
        "Window",
        "Help",
        "Layer",
        "Navigator",
        "Timeline",
    ] {
        assert!(paint.rect_of(text).is_some(), "{text}: {:?}", paint.texts);
    }
    // New and Open sit in the canvas area, between the palettes.
    let page = paint.runtime.shell.page_rect();
    let new = paint.rect_of("New...").unwrap();
    assert!(
        (new.center().x - page.center().x).abs() < 200.0,
        "{new:?} in {page:?}"
    );

    // The Window menu opens a palette that was not showing.
    assert!(paint.rect_of("Color History").is_none());
    paint.click_text("Window");
    assert!(
        paint.rect_of("Hide All Palettes").is_some(),
        "Window opened"
    );
    paint.click_text("Color History");
    paint.settle(10);
    assert!(paint.rect_of("Hide All Palettes").is_none(), "it closed");
    assert!(
        paint.rect_of("Color History").is_some(),
        "the palette shows: {:?}",
        paint.texts
    );

    // New asks for the canvas; Open asks the system's dialog.
    paint.click_text("New...");
    assert!(
        paint.runtime.applets.captures_keys(),
        "the New dialog shows"
    );
    assert!(go(&mut paint, "dialog/close"));
    paint.settle(10);
    assert!(paint.runtime.pending_pick().is_none());
    paint.click_text("Open...");
    assert!(
        paint.runtime.pending_pick().is_some(),
        "Open asked for a file"
    );
    assert_eq!(pictures(&paint), 1, "the picture is closed, not deleted");
}

/// File, Back to the Hub (and the palette's entry, under the name
/// shortcuts saved for the launcher's Start Page) go to the Hub; the
/// launcher's routes are gone.
#[test]
fn back_to_the_hub_replaces_the_start_page_and_its_routes_are_gone() {
    let mut paint = PaintDriver::new("paint-back-to-hub");
    paint.command("file.start_page");
    paint.settle(80);
    assert!(paint.runtime.shell.page().is_none(), "the Hub shows");
    assert!(
        paint
            .runtime
            .shell
            .message()
            .is_none_or(|(_, error)| !error)
    );
    assert_eq!(pictures(&paint), 1, "the canvas stays open in Sue Paint");
    // The palette offers it by its new name.
    paint
        .runtime
        .request(ShellRequest::Palette, &mut NoTextures, paint.now);
    for letter in ["b", "a", "c", "k", " ", "t", "o"] {
        paint.step(vec![egui::Event::Text(letter.to_owned())]);
    }
    paint.settle(4);
    assert!(
        paint.rect_of("Sue Paint: Back to the Hub").is_some(),
        "{:?}",
        paint.texts
    );
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    for gone in [
        "start/projects",
        "start/materials",
        "start/transfers",
        "start/help",
        "start/settings",
        "start/maintenance",
        "start/show",
        "start/new",
        "start/new/2",
    ] {
        assert!(!go(&mut paint, gone), "{gone} still answers");
    }
}

/// On a phone, Sue Paint with no canvas shows its canvas area with New
/// and Open; New opens the New canvas page, and the back button goes to
/// the Hub, whose tab bar has the Materials place.
#[test]
fn on_a_phone_new_opens_the_new_canvas_page_and_back_opens_the_hub() {
    let (mut runtime, root) = runtime("paint-phone-flow");
    let ctx = egui::Context::default();
    let size = vec2(430.0, 932.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
    assert!(
        runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Layout", "Phone"], io, now)
        })
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(runtime.shell.form_factor(), FormFactor::Mobile);
    runtime.request(ShellRequest::Open(paint_id()), &mut NoTextures, start);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let draw = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, start));
        let texts = text_rects(&output);
        output.drop_without_applying_deltas();
        texts
    };
    let mut texts = Vec::new();
    for _ in 0..30 {
        texts = draw(&mut runtime, Vec::new());
    }
    let find = |texts: &[(String, egui::Rect)], name: &str| {
        texts
            .iter()
            .find(|(text, _)| text == name)
            .map(|(_, rect)| *rect)
    };
    assert!(find(&texts, "No canvas is open").is_some(), "{texts:?}");
    let new = find(&texts, "New...").expect("New in the canvas area");
    assert!(find(&texts, "Open...").is_some());
    for event in [
        vec![egui::Event::PointerMoved(new.center())],
        vec![egui::Event::PointerButton {
            pos: new.center(),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![egui::Event::PointerButton {
            pos: new.center(),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        draw(&mut runtime, event);
    }
    for _ in 0..6 {
        texts = draw(&mut runtime, Vec::new());
    }
    assert!(find(&texts, "New canvas").is_some(), "{texts:?}");
    assert!(find(&texts, "Advanced Settings").is_some());
    assert!(!runtime.applets.captures_keys(), "no dialog on a phone");
    // The phone's back button runs Back to the Hub.
    runtime
        .applets
        .get_mut(&paint_id())
        .unwrap()
        .navigate("new-canvas/close");
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint_id(),
            command: "file.start_page".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(
        runtime.shell.hub_open(),
        "the Hub shows: {:?} {:?}",
        runtime.shell.page(),
        runtime.shell.message()
    );
    let materials = runtime
        .shell
        .hub_buttons()
        .iter()
        .find(|(button, _)| *button == sue_wm::HubButton::Place("materials".to_owned()))
        .map(|(_, rect)| *rect)
        .expect("the Materials place in the tab bar");
    assert!(materials.bottom() > size.y - 2.0, "{materials:?}");
    frames(
        &mut runtime,
        &ctx,
        size,
        start,
        click_at(materials.center()),
    );
    assert_eq!(runtime.shell.hub_place(), Some("materials"));
    for _ in 0..6 {
        texts = draw(&mut runtime, Vec::new());
    }
    assert!(find(&texts, "Search materials").is_some(), "{texts:?}");
    // The kinds are a row of chips on a phone.
    assert!(find(&texts, "All").is_some(), "{texts:?}");
    assert!(find(&texts, "All materials").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// The Materials place lists Sue Paint's materials, and its menu finds
/// them again, draws their pictures again, shows and moves their folder
/// (what the launcher's Settings and Maintenance popups did).
#[test]
fn the_materials_place_and_its_menu_find_redraw_show_and_move_them() {
    let mut paint = PaintDriver::new("paint-materials-place");
    to_materials(&mut paint);
    for text in [
        "Materials",
        "All materials",
        "Favorite",
        "Recently used",
        "Sue Paint's own",
        "Added by you",
        "Add a material",
    ] {
        assert!(paint.rect_of(text).is_some(), "{text}: {:?}", paint.texts);
    }
    let menu = |paint: &mut PaintDriver, entry: &str| {
        assert!(go(paint, "materials/menu"));
        paint.settle(6);
        paint.click_text(entry);
        paint.settle(6);
    };
    menu(&mut paint, "Find Materials Again");
    assert_eq!(notice(&paint), "The materials were looked through again");
    menu(&mut paint, "Rebuild Thumbnails");
    assert_eq!(
        notice(&paint),
        "The pictures of the materials are drawn again"
    );
    menu(&mut paint, "Show Materials Folder");
    assert!(notice(&paint).contains("materials"), "{}", notice(&paint));

    // Another location, picked with the system's folder dialog.
    menu(&mut paint, "Choose Location of Materials...");
    let pick = paint.runtime.pending_pick().expect("a folder is asked for");
    assert_eq!(pick.purpose, "materials-location");
    let elsewhere = paint.root.join("my materials");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert!(
        paint
            .runtime
            .answer_pick(&elsewhere, &mut NoTextures, paint.now)
    );
    paint.settle(6);
    let location = |paint: &PaintDriver| {
        paint
            .runtime
            .engine
            .world
            .rice
            .get_str("sue_paint.materials_dir")
            .unwrap_or("")
            .to_owned()
    };
    assert_eq!(location(&paint), elsewhere.display().to_string());
    // And back to Sue Paint's own folder.
    menu(&mut paint, "Use Sue Paint's Own Folder");
    assert_eq!(location(&paint), "");
}

/// A picture added in the Materials place is copied into the materials
/// folder and listed; a click on it while a canvas is open goes to the
/// canvas and pastes it there as a layer, one undo step; its popup
/// deletes it in two clicks.
#[test]
fn a_material_added_in_the_hub_is_listed_pasted_on_the_canvas_and_deleted() {
    let mut paint = PaintDriver::new("paint-materials-add");
    std::fs::create_dir_all(&paint.root).unwrap();
    let picture = paint.root.join("Starfish.png");
    let pixels = vec![200_u8; 16 * 12 * 4];
    std::fs::write(&picture, sue_io::encode_png_rgba(16, 12, &pixels).unwrap()).unwrap();
    to_materials(&mut paint);
    paint.click_text("Add a material");
    let pick = paint
        .runtime
        .pending_pick()
        .expect("a picture is asked for");
    assert_eq!(pick.purpose, "material-add");
    assert!(
        paint
            .runtime
            .answer_pick(&picture, &mut NoTextures, paint.now)
    );
    // The copy runs in the background, into the materials folder; the
    // place lists it once done.
    let begun = Instant::now();
    while paint.rect_of("Starfish").is_none() {
        assert!(
            begun.elapsed().as_secs() < 30,
            "the copy is listed: {:?}",
            paint.texts
        );
        std::thread::sleep(Duration::from_millis(10));
        // What the runtime's tick does between frames.
        paint.runtime.transfers.poll();
        paint.settle(2);
    }

    assert_eq!(starfish(&paint.root), 2, "a copy in the materials folder");
    // A click with the canvas open puts it on the canvas, in Sue Paint.
    let before = layer_names(&paint);
    let arena_before = entities(&paint);
    paint.click_text("Starfish");
    paint.settle(80);
    assert_eq!(
        paint.runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID),
        "Sue Paint shows"
    );
    let after = layer_names(&paint);
    let count = |names: &[String]| names.iter().filter(|name| *name == "Starfish").count();
    assert_eq!(count(&after), count(&before) + 1, "{after:?}");
    let arena_after = entities(&paint);
    // One undo step on the canvas's own history gives back the arena as
    // it was, ids included; redo the arena with the paste.
    paint.command("edit.undo");
    paint.settle(4);
    assert!(entities(&paint) == arena_before, "undo restores the arena");
    paint.command("edit.redo");
    paint.settle(4);
    assert!(entities(&paint) == arena_after, "redo restores the paste");

    // Its popup: Delete, then Really delete.
    to_materials(&mut paint);
    right_click_text(&mut paint, "Starfish");
    paint.click_text("Details");
    paint.settle(6);
    assert!(paint.rect_of("Kind").is_some(), "{:?}", paint.texts);
    paint.click_text("Delete the chosen materials");
    paint.click_text("Really delete the chosen materials");
    paint.settle(10);
    assert!(paint.rect_of("Starfish").is_none(), "{:?}", paint.texts);
    assert_eq!(starfish(&paint.root), 1, "only the picture it came from");
}

/// How many files under `dir` are called Starfish.
fn starfish(dir: &std::path::Path) -> usize {
    walk(dir)
        .iter()
        .filter(|name| name.starts_with("Starfish"))
        .count()
}

/// Every file name under `dir`.
fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    found
}

/// What the launcher's Help popup held is in Sue Paint's Help menu: the
/// tips, the guide online, frequently asked questions, contact and feedback,
/// each opening its page.
#[test]
fn the_help_menu_opens_what_the_launchers_help_popup_held() {
    let mut paint = PaintDriver::new("paint-help-links");
    paint.click_text("Help");
    paint.click_text("Frequently Asked Questions");
    assert!(
        notice(&paint).contains("issues?q=label%3Aquestion"),
        "{}",
        notice(&paint)
    );
    for (command, page) in [
        ("help.tips", "magigun.github.io/sue"),
        ("help.online_guide", "HUMANS.md#sue-paint"),
        ("help.contact", "issues/new/choose"),
        ("help.feedback", "template=feature.yml"),
    ] {
        paint.command(command);
        paint.settle(2);
        assert!(
            notice(&paint).contains(page),
            "{command}: {}",
            notice(&paint)
        );
    }
}

/// What the launcher's Settings popup held is in Settings, Sue Paint:
/// the colors (its Dark mode), where the materials are, and the pen
/// pressure curve (File, Pen Pressure Settings draws it).
#[test]
fn the_launchers_settings_are_rows_of_sue_paints_settings() {
    let (runtime, root) = runtime("paint-settings-rows");
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let rice = &runtime.engine.world.rice;
    let pages = crate::settings::all_pages(&runtime.engine.catalog, rice);
    // Each key is a setting Sue knows, and Settings shows it in the
    // group named Sue Paint (by the key itself or by its section).
    let groups: Vec<&sue_wm::SettingsGroup> = pages
        .iter()
        .flat_map(|page| &page.groups)
        .filter(|group| group.title == "Sue Paint")
        .collect();
    assert!(!groups.is_empty(), "Settings has a Sue Paint group");
    for key in [
        "sue_paint.theme",
        "sue_paint.materials_dir",
        "sue_paint.pen_pressure",
    ] {
        let entry = rice
            .entry(key)
            .unwrap_or_else(|| panic!("{key} is a setting"));
        assert!(
            groups
                .iter()
                .flat_map(|group| &group.items)
                .any(|item| match item {
                    sue_wm::SettingsItem::Param(path) => path == key,
                    sue_wm::SettingsItem::Section(section) => section == entry.section(),
                    _ => false,
                }),
            "{key} is a row of Settings, Sue Paint"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
