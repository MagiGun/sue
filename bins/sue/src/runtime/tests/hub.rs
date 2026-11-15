//! The Hub: its places, its cards and buttons, the Files place, pages
//! over it, what a run remembers.

use super::*;

#[test]
fn a_click_on_a_hub_box_glides_its_studio_in_and_then_sleeps() {
    let (mut runtime, root) = runtime("hub");
    let ctx = egui::Context::default();
    let start = Instant::now();
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
        Vec::new()
    });
    assert!(runtime.shell.hub_open());
    // The studios, each a box with its pin. Sue ships no tool; one that
    // a package adds would follow them.
    let boxes: Vec<&str> = runtime
        .shell
        .hub_boxes()
        .iter()
        .map(|(id, _, _)| id.as_str())
        .collect();
    assert_eq!(boxes, [sue_paint::ID, sue_writer::ID]);
    let (rect, _) = hub_box(&runtime, sue_paint::ID).expect("Sue Paint's box");
    let at = rect.left_top() + vec2(30.0, 30.0);
    let count = frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, click_at(at));
    assert!(count > 10, "the page must glide in ({count} frames)");
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID)
    );
    assert!(runtime.shell.reveal() > 0.999);
    assert!(!matches!(
        runtime.pacing(start + Duration::from_secs(60)),
        FramePacing::Animate { .. }
    ));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn pages_slide_in_over_the_hub_and_back_out() {
    let (mut runtime, root) = runtime("page-motion");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(runtime.shell.hub_open());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    // New, Picture, Square Post: Sue Paint makes it and opens on it,
    // coming in over the Hub.
    runtime.request(
        ShellRequest::Create {
            applet: paint.clone(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    assert_eq!(runtime.shell.page(), Some(&paint));
    assert!(runtime.shell.reveal() < 0.01, "it starts on the Hub");
    let opening = frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(opening > 10, "coming in takes a moment ({opening} frames)");
    assert!(runtime.shell.reveal() > 0.999);
    assert!(
        runtime.shell.hub_boxes().is_empty(),
        "the Hub is not drawn under a page in place"
    );
    assert!(
        runtime
            .engine
            .world
            .arena
            .iter()
            .any(|(_, entity)| sue_model::picture::is_picture(entity)),
        "the picture was made"
    );

    runtime.request(ShellRequest::Hub, &mut NoTextures, start);
    assert!(runtime.shell.hub_open(), "leaving starts at once");
    let closing = frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(closing > 10, "going out takes a moment ({closing})");
    assert!(runtime.shell.reveal() < 0.001);
    assert_eq!(
        runtime.shell.hub_boxes().len(),
        2,
        "the Hub shows both studios again"
    );

    // With instant transitions the same trip takes a frame or two.
    runtime
        .engine
        .history
        .apply(
            &mut runtime.engine.world,
            RiceEdit(sue_rice::SetRiceValue::new(
                "motion.instant_transitions",
                sue_rice::RiceValue::Bool(true),
            )),
        )
        .unwrap();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(ShellRequest::Open(paint), &mut NoTextures, start);
    for frame in 0..2 {
        frame_with(
            &mut runtime,
            &ctx,
            start + Duration::from_millis(8 * frame),
            Vec::new(),
        );
    }
    assert!(
        runtime.shell.reveal() > 0.999,
        "instant transitions snap in a frame or two"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_hub_lists_recent_files_saved_projects_and_unsaved_work() {
    use sue_wm::HubRow;
    let (host, root) = temp_host("hub-lists");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let boot = || {
        Runtime::new(Engine::boot(&host).unwrap())
            .unwrap()
            .remembering(true)
    };
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let pictures = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::picture::is_picture(entity))
            .count()
    };
    let files = |runtime: &mut Runtime| {
        runtime.request(
            ShellRequest::HubPlace(sue_wm::HUB_FILES.to_owned()),
            &mut NoTextures,
            start,
        );
        frames(runtime, &ctx, size, start, |_| Vec::new());
    };
    let mut runtime = boot();
    files(&mut runtime);
    assert_eq!(runtime.shell.hub_place(), Some(sue_wm::HUB_FILES));
    assert!(runtime.shell.hub_rows().is_empty(), "nothing yet");

    // A picture opened from a file goes first among the recent files. It
    // is a canvas of Sue Paint's.
    std::fs::create_dir_all(&root).unwrap();
    let photo = root.join("photo.png");
    let pixels = vec![200_u8; 4 * 4 * 4];
    std::fs::write(&photo, sue_io::encode_png_rgba(4, 4, &pixels).unwrap()).unwrap();
    runtime.request(
        ShellRequest::OpenFile(photo.clone()),
        &mut NoTextures,
        start,
    );
    assert_eq!(
        runtime.work().recent().next().map(|file| &file.path),
        Some(&photo)
    );

    // A picture made in Sue Paint and saved as a project is a saved
    // project too.
    runtime.request(
        ShellRequest::Create {
            applet: paint.clone(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let album = root.join("album.sue");
    runtime.save_project_to(&album).unwrap();
    let first: Vec<&Path> = runtime
        .work()
        .recent()
        .map(|file| file.path.as_path())
        .collect();
    assert_eq!(first, [album.as_path(), photo.as_path()]);
    assert_eq!(runtime.work().projects().len(), 1);
    assert_eq!(runtime.work().projects()[0].page.as_ref(), Some(&paint));
    // The Sue menu opens them too.
    runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
    assert_eq!(
        runtime.shell.sue_menu_labels(&["File", "Open Recent"]),
        ["album.sue", "photo.png"]
    );
    runtime.shell.dismiss_overlays();

    // A second picture, never saved, is kept at quit.
    runtime.request(
        ShellRequest::Create {
            applet: paint.clone(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    assert_eq!(pictures(&runtime), 3);
    runtime.shutdown();

    // The next start lists all three in the Hub's Files.
    let mut runtime = boot();
    files(&mut runtime);
    let kept = runtime.work().unsaved().next().cloned().unwrap();
    assert_eq!(kept.project.as_deref(), Some(album.as_path()));
    assert_eq!(kept.title(), "album");
    for row in [
        HubRow::Open(photo.clone()),
        HubRow::Open(album.clone()),
        HubRow::Recover(kept.id.clone()),
        HubRow::Discard(kept.id.clone()),
    ] {
        assert!(hub_row(&runtime, &row).is_some(), "{row:?} is on the Hub");
    }

    // A click on its card brings it back in Sue Paint, every picture, and
    // the project it belongs to.
    let at = hub_row(&runtime, &HubRow::Recover(kept.id.clone()))
        .unwrap()
        .left_center()
        + vec2(40.0, 0.0);
    frames(&mut runtime, &ctx, size, start, click_at(at));
    assert_eq!(runtime.shell.page(), Some(&paint));
    assert_eq!(pictures(&runtime), 3);
    assert_eq!(runtime.project(), Some(album.as_path()));
    assert_eq!(runtime.work().unsaved().count(), 0, "it is this run's now");
    assert!(kept.file.exists(), "kept until it is saved");
    runtime.save_project_to(&album).unwrap();
    assert!(!kept.file.exists(), "saved, it is let go");

    // Opening another project keeps the unsaved work it replaces.
    runtime.request(
        ShellRequest::Create {
            applet: paint.clone(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    assert_eq!(pictures(&runtime), 4);
    runtime.request(
        ShellRequest::OpenFile(album.clone()),
        &mut NoTextures,
        start,
    );
    assert_eq!(pictures(&runtime), 3, "the saved project");
    assert_eq!(
        runtime.shell.page(),
        Some(&paint),
        "in the studio it was saved from"
    );
    let replaced = runtime.work().unsaved().next().cloned().unwrap();

    // Discard, on its card, asks once more, then throws it away.
    runtime.request(ShellRequest::Hub, &mut NoTextures, start);
    files(&mut runtime);
    let discard = HubRow::Discard(replaced.id.clone());
    let button = hub_row(&runtime, &discard).unwrap().center();
    frames(&mut runtime, &ctx, size, start, click_at(button));
    assert!(replaced.file.exists(), "a first click only asks");
    let button = hub_row(&runtime, &discard).unwrap().center();
    frames(&mut runtime, &ctx, size, start, click_at(button));
    assert!(!replaced.file.exists());
    assert_eq!(runtime.work().unsaved().count(), 0);
    assert!(hub_row(&runtime, &discard).is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn runs_that_do_not_remember_leave_no_trace() {
    let (mut runtime, root) = runtime("no-trace");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("story.sue");
    runtime.request(
        ShellRequest::Create {
            applet: AppletId::new(sue_paint::ID).unwrap(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        Instant::now(),
    );
    runtime.save_project_to(&path).unwrap();
    runtime.shutdown();
    assert!(!runtime.engine.paths.data_dir.join("work").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pinned_studio_opens_at_launch_and_the_pin_undoes() {
    let (host, root) = temp_host("pin");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let mut runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(runtime.shell.hub_open(), "nothing pinned: the Hub");
    let pinned = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .rice
            .get_str(sue_wm::PIN_KEY)
            .unwrap_or_default()
            .to_owned()
    };
    // A click on Sue Writer's pin, at the top right of its card.
    let (rect, pin) = hub_box(&runtime, sue_writer::ID).expect("Sue Writer's box");
    assert!(
        (pin.right() - rect.right()).abs() < 20.0 && (pin.top() - rect.top()).abs() < 20.0,
        "the pin sits at the card's top right"
    );
    frames(&mut runtime, &ctx, size, start, click_at(pin.center()));
    assert_eq!(pinned(&runtime), sue_writer::ID);
    assert!(runtime.shell.hub_open(), "pinning does not open it");
    // One undo step, like any other change.
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(pinned(&runtime), "");
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(pinned(&runtime), sue_writer::ID);
    // One pin at a time: Sue Paint's pin takes it over.
    let paint_pin = hub_box(&runtime, sue_paint::ID)
        .expect("Sue Paint's box")
        .1
        .center();
    frames(&mut runtime, &ctx, size, start, click_at(paint_pin));
    assert_eq!(pinned(&runtime), sue_paint::ID);
    frames(&mut runtime, &ctx, size, start, click_at(pin.center()));
    assert_eq!(pinned(&runtime), sue_writer::ID);
    // The rice file keeps it.
    let rice = std::fs::read_to_string(runtime.engine.rice_path()).unwrap();
    assert!(rice.contains("pinned = \"sue-writer\""), "{rice}");

    // Started again, Sue opens straight into Sue Writer, already in place.
    let mut again = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    frame_with(&mut again, &ctx, start, Vec::new());
    assert_eq!(
        again.shell.page().map(AppletId::as_str),
        Some(sue_writer::ID)
    );
    assert!(again.shell.reveal() > 0.999, "no trip through the Hub");
    // Its Sue menu takes the pin away; the next start is on the Hub.
    again.request(ShellRequest::SueMenu, &mut NoTextures, start);
    let unpinned = again.with_shell(&mut NoTextures, start, |shell, io, now| {
        shell.sue_menu_pick(&["Open Sue Writer at Launch"], io, now)
    });
    assert!(unpinned);
    frames(&mut again, &ctx, size, start, |_| Vec::new());
    let third = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    let mut third = third;
    frame_with(&mut third, &ctx, start, Vec::new());
    assert!(third.shell.hub_open());
    let _ = std::fs::remove_dir_all(&root);
}

/// Where the Hub drew `button` in the last frame.
fn hub_button(runtime: &Runtime, button: &sue_wm::HubButton) -> Option<egui::Rect> {
    runtime
        .shell
        .hub_buttons()
        .iter()
        .find(|(drawn, _)| drawn == button)
        .map(|(_, rect)| *rect)
}

#[test]
fn the_hub_is_a_rail_of_places_and_your_work_opens_files_already_sorted() {
    use sue_wm::{HubButton, WorkShelf};
    let (mut runtime, root) = runtime("hub-places");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // Home and Files sit in a rail at the left, Home chosen.
    let place = |runtime: &Runtime, id: &str| {
        hub_button(runtime, &HubButton::Place(id.to_owned())).expect("a place in the rail")
    };
    let (home, files) = (
        place(&runtime, sue_wm::HUB_HOME),
        place(&runtime, sue_wm::HUB_FILES),
    );
    assert!(
        home.left() < 1.0 && files.top() > home.top(),
        "{home:?} {files:?}"
    );
    assert_eq!(runtime.shell.hub_place(), None, "Home");
    // New, Open, the four shelves of Your work, and the top bar's search,
    // Transfers and Settings.
    for button in [
        HubButton::New,
        HubButton::Open,
        HubButton::Work(WorkShelf::Recent),
        HubButton::Work(WorkShelf::Projects),
        HubButton::Work(WorkShelf::Unsaved),
        HubButton::Work(WorkShelf::Favorites),
        HubButton::Search,
        HubButton::Transfers,
        HubButton::Settings,
    ] {
        assert!(
            hub_button(&runtime, &button).is_some(),
            "{button:?} is on the Hub"
        );
    }
    // Nothing to jump back into on a first run.
    assert!(hub_button(&runtime, &HubButton::AllFiles).is_none());

    // Your work's Unsaved Work opens Files.
    let unsaved = hub_button(&runtime, &HubButton::Work(WorkShelf::Unsaved)).unwrap();
    frames(&mut runtime, &ctx, size, start, click_at(unsaved.center()));
    assert_eq!(runtime.shell.hub_place(), Some(sue_wm::HUB_FILES));
    assert!(
        hub_button(&runtime, &HubButton::Upkeep).is_some(),
        "the Files menu"
    );
    // The rail takes it back Home.
    let home = place(&runtime, sue_wm::HUB_HOME);
    frames(&mut runtime, &ctx, size, start, click_at(home.center()));
    assert_eq!(runtime.shell.hub_place(), None);

    // The search field opens the command palette; Settings its window.
    let search = hub_button(&runtime, &HubButton::Search).unwrap();
    frames(&mut runtime, &ctx, size, start, click_at(search.center()));
    assert!(runtime.shell.palette_open());
    runtime.shell.dismiss_overlays();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let settings = hub_button(&runtime, &HubButton::Settings).unwrap();
    frames(&mut runtime, &ctx, size, start, click_at(settings.center()));
    assert_eq!(
        runtime.shell.window().map(AppletId::as_str),
        Some(crate::settings::ID)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn your_work_opens_files_on_its_shelf() {
    use sue_wm::{HubButton, HubRow, WorkShelf};
    let (host, root) = temp_host("hub-shelves");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let boot = || {
        Runtime::new(Engine::boot(&host).unwrap())
            .unwrap()
            .remembering(true)
    };
    // A picture opened, a project saved, and work kept at quit.
    let mut runtime = boot();
    std::fs::create_dir_all(&root).unwrap();
    let photo = root.join("photo.png");
    std::fs::write(
        &photo,
        sue_io::encode_png_rgba(2, 2, &[120_u8; 16]).unwrap(),
    )
    .unwrap();
    runtime.request(
        ShellRequest::OpenFile(photo.clone()),
        &mut NoTextures,
        start,
    );
    let paint = AppletId::new(sue_paint::ID).unwrap();
    runtime.request(
        ShellRequest::Create {
            applet: paint.clone(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let album = root.join("album.sue");
    runtime.save_project_to(&album).unwrap();
    runtime.request(
        ShellRequest::Create {
            applet: paint,
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    runtime.shutdown();
    let mut runtime = boot();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let kept = runtime.work().unsaved().next().cloned().unwrap();
    // Jump back in holds the two files, newest first; All Files opens
    // Files on everything.
    let jumps: Vec<_> = runtime
        .shell
        .hub_buttons()
        .iter()
        .filter_map(|(button, _)| match button {
            HubButton::Jump(path) => Some(path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(jumps, [album.clone(), photo.clone()]);
    let shown = |runtime: &Runtime| {
        let mut rows: Vec<String> = runtime
            .shell
            .hub_rows()
            .iter()
            .map(|(row, _)| match row {
                HubRow::Open(path) => path.display().to_string(),
                HubRow::Recover(id) | HubRow::Discard(id) => id.clone(),
            })
            .collect();
        rows.dedup();
        rows
    };
    let all = hub_button(&runtime, &HubButton::AllFiles).unwrap();
    frames(&mut runtime, &ctx, size, start, click_at(all.center()));
    assert_eq!(runtime.shell.hub_place(), Some(sue_wm::HUB_FILES));
    assert_eq!(shown(&runtime).len(), 3, "{:?}", shown(&runtime));
    // Each shelf of Your work shows its own.
    for (shelf, expected) in [
        (WorkShelf::Projects, vec![album.display().to_string()]),
        (WorkShelf::Unsaved, vec![kept.id.clone()]),
        (WorkShelf::Favorites, Vec::new()),
    ] {
        runtime.request(ShellRequest::Hub, &mut NoTextures, start);
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let button = hub_button(&runtime, &HubButton::Work(shelf)).unwrap();
        frames(&mut runtime, &ctx, size, start, click_at(button.center()));
        assert_eq!(runtime.shell.hub_place(), Some(sue_wm::HUB_FILES));
        assert_eq!(shown(&runtime), expected, "{shelf:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn on_a_narrow_screen_the_places_are_a_tab_bar_and_the_cards_rows() {
    use sue_wm::HubButton;
    let (mut runtime, root) = runtime("hub-narrow");
    let ctx = egui::Context::default();
    let size = vec2(420.0, 860.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let place = |id: &str| {
        hub_button(&runtime, &HubButton::Place(id.to_owned())).expect("a place in the tab bar")
    };
    let (home, files) = (place(sue_wm::HUB_HOME), place(sue_wm::HUB_FILES));
    assert!(home.bottom() > size.y - 2.0, "along the bottom: {home:?}");
    assert!((home.top() - files.top()).abs() < 0.5 && files.left() > home.left());
    // A studio's card spans the page, its pin at its end.
    let (card, pin) = hub_box(&runtime, sue_paint::ID).unwrap();
    assert!(card.width() > size.x - 60.0, "{card:?}");
    assert!(
        (pin.center().y - card.center().y).abs() < 1.0,
        "the pin at its end"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// One frame with no input, `dt` seconds after the last; returns how long
/// egui asks to wait before the next.
fn idle_frame(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    size: egui::Vec2,
    now: Instant,
) -> Duration {
    let time = ctx.input(|input| input.time) + 0.016;
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(time),
        ..egui::RawInput::default()
    };
    let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map_or(Duration::ZERO, |viewport| viewport.repaint_delay)
}

#[test]
fn an_idle_hub_asks_for_no_frame() {
    let (mut runtime, root) = runtime("hub-idle");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // The pointer crosses a studio's card and leaves: its hover fades in
    // and out, asking for frames while it moves, then nothing moves.
    let (card, _) = hub_box(&runtime, sue_paint::ID).unwrap();
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        0 => vec![egui::Event::PointerMoved(card.center())],
        _ => Vec::new(),
    });
    let fading = idle_frame(&mut runtime, &ctx, size, start);
    assert!(fading < Duration::from_secs(1), "the hover fades in");
    let mut gone = vec![egui::Event::PointerGone];
    let mut delay = Duration::ZERO;
    for _ in 0..120 {
        let time = ctx.input(|input| input.time) + 0.016;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(time),
            events: std::mem::take(&mut gone),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, start));
        delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::ZERO, |viewport| viewport.repaint_delay);
        if delay == Duration::MAX {
            break;
        }
    }
    assert_eq!(delay, Duration::MAX, "the idle Hub asks for no frame");
    assert_eq!(idle_frame(&mut runtime, &ctx, size, start), Duration::MAX);
    assert!(!matches!(
        runtime.pacing(start + Duration::from_secs(60)),
        FramePacing::Animate { .. }
    ));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn with_instant_transitions_the_hub_hover_snaps() {
    let (mut runtime, root) = runtime("hub-instant");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    runtime
        .engine
        .history
        .apply(
            &mut runtime.engine.world,
            RiceEdit(sue_rice::SetRiceValue::new(
                "motion.instant_transitions",
                sue_rice::RiceValue::Bool(true),
            )),
        )
        .unwrap();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let (card, _) = hub_box(&runtime, sue_paint::ID).unwrap();
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        0 => vec![egui::Event::PointerMoved(card.center())],
        _ => Vec::new(),
    });
    assert_eq!(
        idle_frame(&mut runtime, &ctx, size, start),
        Duration::MAX,
        "nothing fades: the hover is there at once"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_files_menu_rebuilds_thumbnails_forgets_what_is_gone_and_clears_transfers() {
    let (host, root) = temp_host("hub-upkeep");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let mut runtime = Runtime::new(Engine::boot(&host).unwrap())
        .unwrap()
        .remembering(true);
    std::fs::create_dir_all(&root).unwrap();
    let kept = root.join("kept.png");
    let gone = root.join("gone.png");
    for path in [&kept, &gone] {
        std::fs::write(path, sue_io::encode_png_rgba(2, 2, &[90_u8; 16]).unwrap()).unwrap();
        runtime.request(ShellRequest::OpenFile(path.clone()), &mut NoTextures, start);
    }
    runtime.request(ShellRequest::Hub, &mut NoTextures, start);
    runtime.request(
        ShellRequest::HubPlace(sue_wm::HUB_FILES.to_owned()),
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(hub_row(&runtime, &sue_wm::HubRow::Open(gone.clone())).is_some());
    std::fs::remove_file(&gone).unwrap();
    let pick = |runtime: &mut Runtime, label: &str| {
        let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
            shell.hub_files_menu_pick(label, io)
        });
        assert!(picked, "the Files menu has {label}");
    };
    // Forget Files That Are Gone: the gone file leaves the list, the
    // other stays.
    pick(&mut runtime, "Forget Files That Are Gone");
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let paths: Vec<_> = runtime
        .work()
        .recent()
        .map(|file| file.path.clone())
        .collect();
    assert_eq!(paths, std::slice::from_ref(&kept));
    assert!(hub_row(&runtime, &sue_wm::HubRow::Open(gone)).is_none());
    // Rebuild Thumbnails forgets the pictures made, makes them again,
    // and says so.
    for _ in 0..400 {
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        if runtime.shell.hub_has_thumbnail(&kept) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(runtime.shell.hub_has_thumbnail(&kept), "made once");
    pick(&mut runtime, "Rebuild Thumbnails");
    assert!(!runtime.shell.hub_has_thumbnail(&kept), "forgotten");
    for _ in 0..400 {
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        if runtime.shell.hub_has_thumbnail(&kept) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(runtime.shell.hub_has_thumbnail(&kept), "made again");
    assert_eq!(
        runtime.shell.message().map(|(text, _)| text.to_owned()),
        Some("The thumbnails are drawn again".to_owned())
    );
    // Clear Completed Transfers empties the list of what is done.
    runtime
        .transfers
        .done(sue_wm::TransferKind::Import, "kept.png", Some(kept));
    assert!(!runtime.transfers.list().is_empty());
    pick(&mut runtime, "Clear Completed Transfers");
    assert!(
        runtime
            .transfers
            .list()
            .iter()
            .all(|transfer| !transfer.finished()),
        "only what still runs stays"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_keyboard_walks_the_hub_and_enter_uses_what_has_focus() {
    let (mut runtime, root) = runtime("hub-keys");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // Tab is the command palette's; the arrow keys walk the Hub, from the
    // place shown: Home, then Files. Enter uses what has focus.
    let press = |runtime: &mut Runtime, key_pressed: egui::Key| {
        frames(runtime, &ctx, size, start, |frame| {
            if frame == 0 {
                vec![key(key_pressed, egui::Modifiers::NONE)]
            } else {
                Vec::new()
            }
        });
    };
    press(&mut runtime, egui::Key::ArrowDown);
    press(&mut runtime, egui::Key::Enter);
    assert_eq!(runtime.shell.hub_place(), Some(sue_wm::HUB_FILES));
    assert!(!runtime.shell.palette_open());
    // In Files the walk goes on past the rail (Sue Paint's Materials
    // place is the last of it) to the place's menu.
    press(&mut runtime, egui::Key::ArrowDown);
    press(&mut runtime, egui::Key::ArrowDown);
    let focused = ctx
        .memory(|memory| memory.focused())
        .and_then(|id| ctx.read_response(id))
        .map(|response| response.rect);
    assert_eq!(
        focused,
        hub_button(&runtime, &sue_wm::HubButton::Upkeep),
        "the Files menu has focus"
    );
    // And back up to Home.
    press(&mut runtime, egui::Key::ArrowUp);
    press(&mut runtime, egui::Key::ArrowUp);
    press(&mut runtime, egui::Key::ArrowUp);
    press(&mut runtime, egui::Key::Enter);
    assert_eq!(runtime.shell.hub_place(), None);
    let _ = std::fs::remove_dir_all(&root);
}
