//! The Hub: its boxes, its lists, pages over it, what a run remembers.

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
    let mut runtime = boot();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
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

    // The next start lists all three on the Hub.
    let mut runtime = boot();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
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

    // A click on it brings it back in Sue Paint, every picture, and the
    // project it belongs to.
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

    // Discard asks once more, then throws it away.
    runtime.request(ShellRequest::Hub, &mut NoTextures, start);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
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
    // A click on Sue Writer's pin, at the bottom right of its box.
    let (rect, pin) = hub_box(&runtime, sue_writer::ID).expect("Sue Writer's box");
    assert!(
        (pin.right() - rect.right()).abs() < 12.0 && (pin.bottom() - rect.bottom()).abs() < 12.0,
        "the pin sits at the box's bottom right"
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
