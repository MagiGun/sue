//! The shell's polish left by Phase 3 (P4-19): notices that stay few,
//! never cover a modal dialog and sit above a phone's tab bar, and a Hub
//! Home that is finished and inviting before there is any work in it.

use super::*;
use sue_wm::HubButton;

/// Where the Hub drew `button` in the last frame.
fn button(runtime: &Runtime, button: &HubButton) -> Option<egui::Rect> {
    runtime
        .shell
        .hub_buttons()
        .iter()
        .find(|(drawn, _)| drawn == button)
        .map(|(_, rect)| *rect)
}

/// The kinds of work Start Something New offers, by studio.
fn starts(runtime: &Runtime) -> Vec<(String, String, egui::Rect)> {
    runtime
        .shell
        .hub_buttons()
        .iter()
        .filter_map(|(drawn, rect)| match drawn {
            HubButton::Start(applet, key) => Some((applet.as_str().to_owned(), key.clone(), *rect)),
            _ => None,
        })
        .collect()
}

/// One frame `seconds` after the last one, with no input.
fn later(runtime: &mut Runtime, ctx: &egui::Context, size: egui::Vec2, seconds: f64) {
    let time = ctx.input(|input| input.time) + seconds;
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(time),
        ..egui::RawInput::default()
    };
    ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, Instant::now()))
        .drop_without_applying_deltas();
}

#[test]
fn an_empty_home_offers_work_to_start_and_what_is_good_to_know() {
    let (mut runtime, root) = runtime("polish-home");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(runtime.shell.hub_open());
    assert!(button(&runtime, &HubButton::AllFiles).is_none(), "no work");

    // Start Something New: kinds of work from every studio, in turn.
    let offered = starts(&runtime);
    assert!(offered.len() >= 4, "{offered:?}");
    for studio in [sue_paint::ID, sue_writer::ID] {
        assert!(
            offered.iter().any(|(applet, ..)| applet == studio),
            "{studio} offers a kind of work: {offered:?}"
        );
    }
    // A row per studio, in the order of the studio cards.
    assert_eq!(offered[0].0, sue_paint::ID);
    let first_writer = offered
        .iter()
        .find(|(applet, ..)| applet == sue_writer::ID)
        .map(|(.., rect)| *rect)
        .unwrap();
    assert!(
        offered
            .iter()
            .filter(|(applet, ..)| applet == sue_paint::ID)
            .all(|(.., rect)| rect.bottom() < first_writer.top()),
        "Sue Paint's row, then Sue Writer's"
    );
    assert!(
        offered.iter().all(|(_, key, _)| key != "custom"),
        "what asks first stays in New's menu"
    );
    // Good to Know: three tiles, each a button.
    let tips = [
        HubButton::FindCommands,
        HubButton::OpenFiles,
        HubButton::MakeItYours,
    ]
    .map(|tip| button(&runtime, &tip).unwrap_or_else(|| panic!("{tip:?} on Home")));
    // The page is used down to its lower part: no blank half page.
    let page = runtime.shell.page_rect();
    let lowest = tips.iter().map(|rect| rect.bottom()).fold(0.0, f32::max);
    assert!(
        lowest > page.top() + page.height() * 0.6,
        "{lowest} in {page:?}"
    );
    assert!(lowest <= page.bottom(), "and fits on it: {lowest}");
    // The tiles keep to the page's column and do not overlap.
    let work = button(&runtime, &HubButton::Work(sue_wm::WorkShelf::Recent)).unwrap();
    for (_, _, rect) in &offered {
        assert!(rect.bottom() <= work.top(), "Start comes before Your Work");
    }
    assert!(tips.iter().all(|tip| tip.top() >= work.bottom()));

    // Find Any Command opens the command palette; Make Sue Yours opens
    // Settings.
    frames(&mut runtime, &ctx, size, start, click_at(tips[0].center()));
    assert!(runtime.shell.palette_open());
    runtime.shell.dismiss_overlays();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let yours = button(&runtime, &HubButton::MakeItYours).unwrap();
    frames(&mut runtime, &ctx, size, start, click_at(yours.center()));
    assert_eq!(
        runtime.shell.window().map(AppletId::as_str),
        Some(crate::settings::ID)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_kind_of_work_on_home_starts_it_in_its_studio_with_one_click() {
    let (mut runtime, root) = runtime("polish-start");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let (_, key, rect) = starts(&runtime)
        .into_iter()
        .find(|(applet, ..)| applet == sue_paint::ID)
        .expect("a kind of picture");
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let before = runtime
        .applets
        .get(&paint)
        .map(|applet| applet.window_title(&runtime.engine.world.arena));
    frames(&mut runtime, &ctx, size, start, click_at(rect.center()));
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID),
        "{key} opens Sue Paint"
    );
    let after = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.window_title(&runtime.engine.world.arena));
    assert!(
        after.as_deref().is_some_and(|title| title.contains(&key)),
        "a new {key} is open: {after:?} (was {before:?})"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn home_with_work_to_jump_back_into_leaves_the_first_run_tiles_out() {
    let (mut runtime, root) = runtime("polish-jump");
    std::fs::create_dir_all(&root).unwrap();
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let photo = root.join("photo.png");
    std::fs::write(
        &photo,
        sue_io::encode_png_rgba(2, 2, &[120_u8; 16]).unwrap(),
    )
    .unwrap();
    runtime.request(ShellRequest::OpenFile(photo), &mut NoTextures, start);
    runtime.request(ShellRequest::Hub, &mut NoTextures, start);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert!(runtime.shell.hub_open());
    assert!(button(&runtime, &HubButton::AllFiles).is_some());
    assert!(starts(&runtime).is_empty());
    assert!(button(&runtime, &HubButton::FindCommands).is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn notices_stay_few_and_the_older_ones_fold_behind_the_newest() {
    let (mut runtime, root) = runtime("polish-few");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    for n in 0..5 {
        runtime.shell.notify(format!("Notice {n}"), false);
    }
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(runtime.shell.notices(), 5, "none is lost");
    assert_eq!(runtime.shell.notices_folded(), 2, "three show");
    let area = runtime.shell.notices_area().expect("they show");
    let page = runtime.shell.page_rect();
    assert!(page.contains_rect(area));
    assert!(
        area.height() < page.height() / 2.0,
        "the stack keeps to the bottom: {area:?}"
    );
    // Its button unfolds them all.
    let more = runtime.shell.notices_fold_rect().expect("2 more");
    frames(&mut runtime, &ctx, size, start, click_at(more.center()));
    assert_eq!(runtime.shell.notices_folded(), 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn notices_never_cover_a_modal_dialog_and_wait_for_it_to_close() {
    let (mut runtime, root) = runtime("polish-modal");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(ShellRequest::OpenSettings(None), &mut NoTextures, start);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let modal = ctx
        .memory(|memory| memory.top_modal_layer())
        .expect("Settings is a modal dialog");
    runtime.shell.notify("Saved", false);
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let layer = runtime.shell.notices_layer().expect("the notice is drawn");
    assert!(
        layer.order < modal.order,
        "the notice ({layer:?}) is under the dialog ({modal:?})"
    );
    // Long past a notice's time, it is still there: its clock stands
    // while the dialog is open.
    for _ in 0..3 {
        later(&mut runtime, &ctx, size, 5.0);
    }
    assert_eq!(runtime.shell.notices(), 1, "it waits");
    // The dialog closes: the notice comes over everything again.
    runtime.shell.dismiss_overlays();
    runtime.shell.close_window();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(runtime.shell.window(), None);
    assert_eq!(
        runtime.shell.notices_layer().map(|layer| layer.order),
        Some(egui::Order::Foreground)
    );
    assert_eq!(runtime.shell.notices(), 1);
    for _ in 0..3 {
        later(&mut runtime, &ctx, size, 5.0);
    }
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(runtime.shell.notices(), 0, "then it leaves on its time");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn on_a_phone_one_notice_shows_above_the_tab_bar() {
    let (mut runtime, root) = runtime("polish-phone");
    runtime
        .engine
        .world
        .rice
        .set(
            sue_wm::INTERFACE_KEY,
            sue_rice::RiceValue::Text("phone".to_owned()),
        )
        .unwrap();
    let ctx = egui::Context::default();
    let phone = vec2(430.0, 932.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, phone, start, |_| Vec::new());
    for text in [
        "Sue uses the Tablet interface",
        "Sue uses the Desktop interface",
        "Saved Picture.png",
        "no.such.command is not available",
    ] {
        runtime.shell.notify(text, false);
    }
    frames(&mut runtime, &ctx, phone, start, |_| Vec::new());
    assert_eq!(runtime.shell.notices(), 4);
    assert_eq!(runtime.shell.notices_folded(), 3, "only the newest shows");
    let area = runtime.shell.notices_area().expect("it shows");
    let tabs = runtime
        .shell
        .hub_buttons()
        .iter()
        .filter(|(drawn, _)| matches!(drawn, HubButton::Place(_)))
        .map(|(_, rect)| rect.top())
        .fold(f32::INFINITY, f32::min);
    assert!(area.bottom() <= tabs + 0.5, "{area:?} over tabs at {tabs}");
    let page = runtime.shell.page_rect();
    assert!(
        area.height() < page.height() / 5.0,
        "a small band: {area:?} of {page:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
