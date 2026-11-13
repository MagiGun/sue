//! The shell: Sue's menu, files in and out, keys, Settings, languages.

use super::*;

#[test]
fn saved_files_say_where_they_went_and_show_on_a_click() {
    let (mut runtime, root) = runtime("saved");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("out.jxl");
    std::fs::write(&path, b"jxl").unwrap();
    let start = Instant::now();
    runtime.request(
        ShellRequest::Saved {
            path: path.clone(),
            note: "JPEG XL".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let (text, error) = runtime.shell.message().unwrap();
    assert!(!error);
    assert!(text.contains("out.jxl") && text.contains(&root.display().to_string()));
    assert_eq!(runtime.shell.message_path(), Some(path.as_path()));
    // Headless, showing the file names it instead of opening Explorer.
    runtime.request(
        ShellRequest::Command(format!("reveal-file:{}", path.display())),
        &mut NoTextures,
        start,
    );
    assert!(runtime.shell.message().unwrap().0.starts_with("File: "));
    assert_eq!(runtime.shell.message_path(), None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn files_open_through_the_system_dialog_in_the_studio_or_tool_that_takes_them() {
    let (mut runtime, root) = runtime("pick");
    std::fs::create_dir_all(&root).unwrap();
    let picture = root.join("photo.png");
    let pixels = [255_u8, 0, 0, 255].repeat(16);
    std::fs::write(&picture, sue_io::encode_png_rgba(4, 4, &pixels).unwrap()).unwrap();
    let ctx = egui::Context::default();
    let start = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
        Vec::new()
    });
    let pictures = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::picture::is_picture(entity))
            .count()
    };

    // Sue Paint asks; the system's dialog answers (headless, the
    // script does).
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let pick = FilePick::open(Some(paint.clone()), "open", "Open Picture").extensions(&["png"]);
    runtime.request(ShellRequest::PickFile(pick.clone()), &mut NoTextures, start);
    assert_eq!(runtime.pending_pick(), Some(&pick), "the dialog was asked");
    assert_eq!(runtime.shell.window(), None, "no window of Sue's own");
    let before = pictures(&runtime);
    assert!(runtime.answer_pick(&picture, &mut NoTextures, start));
    assert_eq!(pictures(&runtime), before + 1, "Sue Paint opened it");
    assert_eq!(runtime.shell.page(), Some(&paint));
    assert!(!runtime.answer_pick(&picture, &mut NoTextures, start));

    // Open File (Ctrl+O on the Hub) puts a picture in the Image Viewer.
    runtime.request(ShellRequest::Hub, &mut NoTextures, start);
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::O, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    let asked = runtime.pending_pick().cloned().expect("Ctrl+O asked");
    assert_eq!((asked.requester, asked.title.as_str()), (None, "Open File"));
    assert!(runtime.answer_pick(&picture, &mut NoTextures, start));
    let viewer = AppletId::new(crate::viewer::ID).unwrap();
    assert_eq!(runtime.shell.page(), Some(&viewer));
    let shown = runtime
        .applets
        .get(&viewer)
        .and_then(|applet| applet.focus());
    assert_eq!(
        shown
            .and_then(|id| runtime.engine.world.arena.get_by_id(id).ok())
            .and_then(|entity| entity.meta(sue_io::meta::SOURCE)),
        Some(picture.display().to_string().as_str()),
        "the Image Viewer shows the file"
    );

    // Text goes to Sue Writer.
    let notes = root.join("notes.md");
    std::fs::write(&notes, "# Hello\n\nSome *words*.\n").unwrap();
    runtime.open_path(&notes, &mut NoTextures, start);
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_writer::ID)
    );

    // Nothing opens an unknown file, and it does not stay behind.
    let data = root.join("data.bin");
    std::fs::write(&data, [0_u8, 1, 2, 3, 255, 254, 0, 7].repeat(64)).unwrap();
    let entities = runtime.engine.world.arena.len();
    runtime.open_path(&data, &mut NoTextures, start);
    assert_eq!(runtime.engine.world.arena.len(), entities);
    let (message, error) = runtime.shell.message().unwrap();
    assert!(error && message.contains("data.bin"), "{message}");

    // A folder is asked for as a folder.
    let convert = AppletId::new(crate::convert::ID).unwrap();
    runtime.request(
        ShellRequest::AppletCommand {
            applet: convert.clone(),
            command: "convert.folder".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    assert!(runtime.pending_pick().is_some_and(|pick| pick.folders));
    assert_eq!(runtime.shell.page(), Some(&convert));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn studios_keep_the_keys_they_reserve() {
    let (mut runtime, root) = runtime("reserved-keys");
    let ctx = egui::Context::default();
    let start = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
        Vec::new()
    });
    // On the Hub, Tab opens the command palette.
    frame_with(
        &mut runtime,
        &ctx,
        start,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
    );
    assert!(runtime.shell.palette_open());
    runtime.shell.dismiss_overlays();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
        Vec::new()
    });
    // In Sue Paint, Tab hides the palettes, as in Clip Studio Paint.
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
        Vec::new()
    });
    assert!(runtime.shell.reveal() > 0.999);
    frame_with(
        &mut runtime,
        &ctx,
        start,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
    );
    assert!(!runtime.shell.palette_open(), "Tab stayed with Sue Paint");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_sue_menu_follows_the_context_and_sends_work_between_studios() {
    let (mut runtime, root) = runtime("sue-menu");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());

    // The Sue button sits in the window's top left corner; a click
    // opens the Sue menu.
    let button = runtime.shell.sue_button();
    assert_eq!(button.min, egui::Pos2::ZERO);
    frames(&mut runtime, &ctx, size, start, |frame| {
        let at = button.center();
        match frame {
            0 => vec![egui::Event::PointerMoved(at)],
            1 => vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            2 => vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            _ => Vec::new(),
        }
    });
    assert!(runtime.shell.sue_menu_open());
    let first = runtime.shell.sue_menu_labels(&[]);
    for wanted in [
        "Hub",
        "Studios",
        "Tools",
        "New",
        "File",
        "Project",
        "Command Palette (Tab)",
        "Settings",
        "Quit Sue",
    ] {
        assert!(
            first.iter().any(|label| label == wanted),
            "{wanted} in {first:?}"
        );
    }
    assert!(
        !first.iter().any(|label| label.ends_with("at Launch")),
        "the Hub has nothing to pin"
    );
    assert_eq!(
        runtime.shell.sue_menu_labels(&["Studios"]),
        ["Sue Paint", "Sue Writer"]
    );
    assert_eq!(
        runtime.shell.sue_menu_labels(&["Tools"]),
        ["Image Viewer", "Converter"]
    );
    assert_eq!(
        runtime.shell.sue_menu_labels(&["New"]),
        ["Picture", "Writing"]
    );
    assert!(
        runtime
            .shell
            .sue_menu_labels(&["File"])
            .iter()
            .any(|label| label == "Open File...")
    );

    // New, Picture, Square Post: into Sue Paint on a new picture.
    let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
        shell.sue_menu_pick(&["New", "Picture", "Square Post"], io, now)
    });
    assert!(picked);
    assert!(!runtime.shell.sue_menu_open(), "a pick closes the menu");
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    assert_eq!(runtime.shell.page(), Some(&paint));
    let picture = runtime
        .engine
        .world
        .arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .unwrap();

    // In Sue Paint: Send To offers the others, and the pin.
    runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
    let send = runtime.shell.sue_menu_labels(&["Send To"]);
    assert!(
        send.iter().any(|label| label == "Converter")
            && send.iter().any(|label| label == "Sue Writer")
            && !send.iter().any(|label| label == "Sue Paint"),
        "{send:?}"
    );
    assert!(
        runtime
            .shell
            .sue_menu_labels(&[])
            .iter()
            .any(|label| label == "Open Sue Paint at Launch")
    );

    // The picture goes to the Converter, flattened into a PNG.
    let sent = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
        shell.sue_menu_pick(&["Send To", "Converter"], io, now)
    });
    assert!(sent);
    let convert = AppletId::new(crate::convert::ID).unwrap();
    assert_eq!(runtime.shell.page(), Some(&convert));
    runtime.request(
        ShellRequest::AppletCommand {
            applet: convert,
            command: "convert.run".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let home = runtime.engine.catalog.folders["home"].clone();
    let written = std::fs::read_dir(&home)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "png"))
        .expect("the flattened picture was written");
    let png = sue_io::decode_png(&std::fs::read(&written).unwrap()).unwrap();
    let pixels =
        sue_model::picture::size(runtime.engine.world.arena.get_by_id(picture).unwrap()).unwrap();
    assert_eq!(
        [png.tile.width(), png.tile.height()],
        pixels,
        "the whole picture, flattened"
    );

    // Open In goes back to Sue Paint on the very same picture; the Hub
    // item goes home.
    runtime.request(
        ShellRequest::OpenIn {
            entity: picture,
            applet: paint.clone(),
        },
        &mut NoTextures,
        start,
    );
    assert_eq!(runtime.shell.page(), Some(&paint));
    assert_eq!(
        runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus()),
        Some(picture)
    );
    runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
    let home_picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
        shell.sue_menu_pick(&["Hub"], io, now)
    });
    assert!(home_picked && runtime.shell.hub_open());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_sue_menus_layout_switches_the_hub_and_sue_paint_and_stays() {
    let (host, root) = temp_host("layout");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let mut runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(runtime.shell.form_factor(), FormFactor::Desktop);
    let desktop_top = runtime.shell.page_rect().top();
    let pick = |runtime: &mut Runtime, layout: &str| {
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Layout", layout], io, now)
        });
        assert!(picked, "{layout}");
        frames(runtime, &ctx, size, start, |_| Vec::new());
    };

    // Phone: the Hub and Sue Paint draw the phone layout, with the
    // phone's taller bar, in the same window.
    pick(&mut runtime, "Phone");
    assert_eq!(runtime.shell.form_factor(), FormFactor::Mobile);
    assert!(runtime.shell.hub_open());
    assert!(runtime.shell.page_rect().top() > desktop_top);
    assert_eq!(runtime.shell.page_rect().width(), size.x);
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID)
    );
    assert_eq!(runtime.shell.form_factor(), FormFactor::Mobile);

    // Tablet, then back to the desktop the system detects.
    pick(&mut runtime, "Tablet");
    assert_eq!(runtime.shell.form_factor(), FormFactor::Tablet);
    pick(&mut runtime, "Phone");
    // The rice file keeps it, so it survives a restart.
    let rice = std::fs::read_to_string(runtime.engine.rice_path()).unwrap();
    assert!(rice.contains("interface = \"phone\""), "{rice}");
    let mut again = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    frames(&mut again, &ctx, size, start, |_| Vec::new());
    assert_eq!(again.shell.form_factor(), FormFactor::Mobile);
    pick(&mut again, "Automatic (Desktop)");
    assert_eq!(again.shell.form_factor(), FormFactor::Desktop);
    assert_eq!(again.shell.page_rect().top(), desktop_top);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn settings_fits_each_screen_and_how_it_is_held() {
    let (mut runtime, root) = runtime("settings-sheets");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let sheet = |runtime: &mut Runtime, interface: &str, w: f32, h: f32| {
        runtime
            .engine
            .world
            .rice
            .set(
                sue_wm::INTERFACE_KEY,
                sue_rice::RiceValue::Text(interface.to_owned()),
            )
            .unwrap();
        frames(runtime, &ctx, vec2(w, h), start, |_| Vec::new());
        runtime.request(ShellRequest::OpenSettings(None), &mut NoTextures, start);
        frames(runtime, &ctx, vec2(w, h), start, |_| Vec::new());
        runtime.shell.window_rect().expect("Settings is open")
    };
    // A phone's fills the screen, upright and on its side.
    for (w, h) in [(393.0, 851.0), (851.0, 393.0)] {
        let rect = sheet(&mut runtime, "phone", w, h);
        assert_eq!(rect.size(), vec2(w, h), "{rect:?}");
    }
    // A tablet's: nearly the whole width upright, the whole height on
    // its side, and never past the screen.
    let upright = sheet(&mut runtime, "tablet", 800.0, 1280.0);
    assert!(upright.width() > 700.0 && upright.height() < 1280.0 * 0.9);
    let side = sheet(&mut runtime, "tablet", 1280.0, 800.0);
    assert!(side.height() > 700.0 && side.width() < 1280.0 * 0.9);
    // A desktop's keeps a largest size, and fits a small window.
    let big = sheet(&mut runtime, "desktop", 1920.0, 1080.0);
    assert_eq!(big.size(), vec2(980.0, 720.0));
    let small = sheet(&mut runtime, "desktop", 600.0, 420.0);
    let window = egui::Rect::from_min_size(Pos2::ZERO, vec2(600.0, 420.0));
    assert!(window.contains_rect(small), "{small:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_sue_menu_changes_the_language_and_a_tool_can_keep_its_own() {
    let (mut runtime, root) = runtime("language");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());

    // Headless runs start in English; the Sue menu offers every language.
    runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
        shell.open_sue_menu(io)
    });
    let languages = runtime.shell.sue_menu_labels(&["Language"]);
    let japanese = sue_lang::name("ja");
    assert!(languages.contains(&japanese), "{languages:?}");
    assert!(languages.contains(&sue_lang::name("es")), "{languages:?}");

    // Picking Japanese is an undoable rice edit, and the menu speaks it.
    let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
        shell.sue_menu_pick(&["Language", &japanese], io, now)
    });
    assert!(picked);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(
        runtime.engine.world.rice.get_str(sue_wm::LANGUAGE_KEY),
        Some("ja")
    );
    runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
        shell.open_sue_menu(io)
    });
    let labels = runtime.shell.sue_menu_labels(&[]);
    let settings = {
        let _ja = sue_lang::scope("ja");
        sue_lang::text("wm", "settings")
    };
    assert_ne!(settings, "Settings");
    assert!(labels.contains(&settings), "{labels:?}");
    runtime.shell.dismiss_overlays();

    // The Image Viewer set apart in Spanish: what it says is Spanish,
    // while Sue's own menu stays Japanese.
    let viewer = AppletId::new(crate::viewer::ID).unwrap();
    runtime
        .engine
        .world
        .rice
        .set(
            &sue_wm::language_key(&viewer),
            sue_rice::RiceValue::Text("es".to_owned()),
        )
        .unwrap();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(
        ShellRequest::AppletCommand {
            applet: viewer,
            command: "no.such.command".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let spanish = {
        let _es = sue_lang::scope("es");
        sue_lang::tr!("wm", "is-not-available", command = "no.such.command")
    };
    assert_eq!(
        runtime.shell.message().map(|(text, _)| text),
        Some(spanish.as_str())
    );
    runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
        shell.open_sue_menu(io)
    });
    assert!(runtime.shell.sue_menu_labels(&[]).contains(&settings));

    // English again: the menu reads English.
    let _ = runtime.engine.world.rice.set(
        sue_wm::LANGUAGE_KEY,
        sue_rice::RiceValue::Text(sue_lang::ENGLISH.to_owned()),
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
        shell.open_sue_menu(io)
    });
    assert!(
        runtime
            .shell
            .sue_menu_labels(&[])
            .contains(&"Settings".to_owned())
    );
    let _ = std::fs::remove_dir_all(&root);
}
