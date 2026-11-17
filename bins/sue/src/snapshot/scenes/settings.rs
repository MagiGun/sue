//! Scenes of Settings. Every click lands where a frame drew what it
//! clicks, never at a fixed position.

use egui::{Pos2, Rect, vec2};
use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::super::Tour;

/// Every piece of text the next frame draws, with its rect.
fn texts(tour: &mut Tour<'_>) -> Vec<(String, Rect)> {
    let output = tour.camera.frame(&mut tour.runtime);
    tour.camera
        .painter
        .apply_textures(output.textures_delta.clone());
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(text) = shape {
                found.push((
                    text.galley.text().to_owned(),
                    Rect::from_min_size(text.pos, text.galley.size()),
                ));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    found
}

/// Where Settings drew `text` inside its window, the topmost first.
fn rect_of(tour: &mut Tour<'_>, text: &str) -> Result<Rect, String> {
    let window = tour
        .runtime
        .shell
        .window_rect()
        .ok_or("Settings is not open")?;
    texts(tour)
        .into_iter()
        .filter(|(found, rect)| found == text && window.contains(rect.center()))
        .map(|(_, rect)| rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .ok_or_else(|| format!("Settings draws no {text:?}"))
}

/// Clicks the middle of `text` in Settings.
fn click(tour: &mut Tour<'_>, text: &str) -> Result<(), String> {
    let at = rect_of(tour, text)?.center();
    tour.camera.click(at);
    tour.settle();
    Ok(())
}

/// Turns the mouse wheel over Settings' page by `by` points.
fn scroll(tour: &mut Tour<'_>, by: f32) {
    if let Some(window) = tour.runtime.shell.window_rect() {
        let at = window.center() + vec2(window.width() * 0.15, 0.0);
        tour.camera.input.push(vec![egui::Event::PointerMoved(at)]);
        tour.camera.input.push(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -by),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        }]);
        tour.settle();
    }
}

/// Scrolls Settings' page down until it draws `text` in the window's
/// lower half (or it cannot scroll further).
fn scroll_to(tour: &mut Tour<'_>, text: &str) -> Result<(), String> {
    for _ in 0..20 {
        let window = tour
            .runtime
            .shell
            .window_rect()
            .ok_or("Settings is not open")?;
        if rect_of(tour, text).is_ok_and(|rect| rect.bottom() < window.bottom() - 60.0) {
            return Ok(());
        }
        scroll(tour, 160.0);
    }
    rect_of(tour, text).map(|_| ())
}

/// Types `text` into the Settings filter, which is empty.
fn filter(tour: &mut Tour<'_>, text: &str) -> Result<(), String> {
    click(
        tour,
        &sue_lang::text("settings", "filter-settings-ellipsis"),
    )?;
    tour.camera
        .input
        .push(vec![egui::Event::Text(text.to_owned())]);
    tour.settle();
    Ok(())
}

/// 04 and 05. Settings, on two categories; 04b. the color wheel behind
/// the page background's swatch; 04c to 04g. Sue Paint's page, the
/// keyboard shortcuts with a key held twice, the icons, and Sue Writer's
/// page; 04h. the icons with a test pack installed and chosen.
pub(super) fn desktop(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.request(ShellRequest::OpenSettings(Some("appearance".to_owned())));
    tour.shoot("04-settings-appearance")?;
    // The colors wait under Advanced: open them, then the first wheel.
    let colors = sue_lang::text("settings", "colors");
    scroll_to(tour, &colors)?;
    click(tour, &colors)?;
    let row = rect_of(tour, &sue_lang::text("rice", "colors-void"))?;
    let window = tour.runtime.shell.window_rect().ok_or("Settings closed")?;
    // The swatch sits at the row's right end, level with its name.
    tour.camera
        .click(Pos2::new(window.right() - 40.0, row.center().y + 4.0));
    tour.shoot("04b-color-wheel")?;
    tour.camera
        .click(Pos2::new(window.left() + 40.0, window.bottom() - 20.0));
    tour.settle();
    // The icons: Sue's own, as a sheet.
    tour.request(ShellRequest::OpenSettings(Some("appearance".to_owned())));
    let sheet = sue_lang::text("settings", "icons-show-sheet");
    scroll_to(tour, &sheet)?;
    click(tour, &sheet)?;
    tour.shoot("04c-settings-icons")?;
    // 04h. A pack installed from a folder and chosen: its icons, drawn
    // as filled shapes, take the place of Sue's own on the sheet and in
    // the frame around it; then Sue's own again.
    let folder = tour.home.join("snapshot-icon-pack");
    write_test_pack(&folder)?;
    let install = sue_lang::text("settings", "icons-install-folder");
    scroll_to(tour, &install)?;
    click(tour, &install)?;
    let now = tour.camera.now;
    if !tour
        .runtime
        .answer_pick(&folder, &mut tour.camera.painter, now)
    {
        return Err("Install from Folder asked for no folder".to_owned());
    }
    tour.settle();
    // The pack's card, with its own icons as its preview, chosen.
    scroll_to(tour, "Snapshot Shapes")?;
    click(tour, "Snapshot Shapes")?;
    tour.settle();
    tour.shoot("04h-settings-icon-pack")?;
    let sues_own = sue_lang::text("settings", "icons-sues-own");
    scroll(tour, -4000.0);
    scroll_to(tour, &sues_own)?;
    click(tour, &sues_own)?;
    tour.settle();
    // File, Preferences in Sue Paint opens this page.
    tour.request(ShellRequest::OpenSettings(Some(sue_paint::ID.to_owned())));
    tour.shoot("04d-settings-sue-paint")?;
    tour.request(ShellRequest::OpenSettings(Some(sue_writer::ID.to_owned())));
    tour.shoot("04e-settings-sue-writer")?;
    // Keyboard Shortcuts: Sue Paint's Tools, with R given to Bigger Brush
    // as well, so both rows are marked.
    let both: sue_rice::Keybinds = "] or R".parse().map_err(|error| format!("{error}"))?;
    tour.set("sue_paint_keys.brush_bigger", RiceValue::Keybind(both));
    tour.request(ShellRequest::OpenSettings(Some("keymaps".to_owned())));
    tour.shoot("04f-settings-keyboard-shortcuts")?;
    filter(tour, "brush")?;
    tour.shoot("04g-settings-key-held-twice")?;
    let shipped: sue_rice::Keybinds = "]".parse().map_err(|error| format!("{error}"))?;
    tour.set("sue_paint_keys.brush_bigger", RiceValue::Keybind(shipped));
    tour.runtime.shell.close_window();
    tour.request(ShellRequest::OpenSettings(Some("general".to_owned())));
    tour.shoot("05-settings-general")?;
    tour.runtime.shell.close_window();
    tour.runtime.shell.dismiss_overlays();
    updates(tour)
}

/// 05a to 05c. The update card with a newer release and its notes, the
/// notes read whole in a dialog, and a check that failed. No network:
/// the release is made here.
fn updates(tour: &mut Tour<'_>) -> Result<(), String> {
    let release = sue_platform::update::Release {
        tag: "v9.9.9".to_owned(),
        name: "Sue 9.9.9".to_owned(),
        notes: [
            "## What is new",
            "",
            "- Settings and the update window on Sue's own controls.",
            "- Icon packs show their own icons on their cards.",
            "  - Optional filled twins are no longer counted as problems.",
            "",
            "### Fixed",
            "",
            "A **long** line of notes wraps inside the card, and `code` reads plainly;",
            "[the page](https://example.com) reads as its words.",
        ]
        .join("\n"),
        page: "https://example.com/releases/v9.9.9".to_owned(),
        assets: Vec::new(),
        published: None,
    };
    tour.runtime.updater_mut().offer(release);
    tour.settle();
    tour.shoot("05a-update-available")?;
    let notes = sue_lang::text("sue", "update-release-notes");
    let at = texts(tour)
        .into_iter()
        .find(|(text, _)| *text == notes)
        .map(|(_, rect)| rect.center())
        .ok_or("the update card has no Release Notes button")?;
    tour.camera.click(at);
    tour.settle();
    tour.shoot("05b-release-notes")?;
    tour.camera.input.push(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    tour.settle();
    tour.runtime
        .updater_mut()
        .fail("The releases could not be reached (no network).".to_owned());
    tour.settle();
    tour.shoot("05c-update-failed")?;
    let close = sue_lang::text("sue", "update-close");
    if let Some((_, rect)) = texts(tour).into_iter().find(|(text, _)| *text == close) {
        tour.camera.click(rect.center());
    }
    tour.settle();
    Ok(())
}

/// 24 to 26. Settings shaped to each screen: a phone's list and page,
/// upright and on its side; a tablet's two panes, upright and on its
/// side; a small desktop window.
pub(super) fn screens(tour: &mut Tour<'_>) -> Result<(), String> {
    for (interface, size, page, name) in [
        ("phone", [393_u32, 851_u32], "", "24-phone-settings"),
        ("phone", [393, 851], "display", "24a-phone-settings-display"),
        (
            "phone",
            [851, 393],
            "appearance",
            "24b-phone-on-its-side-settings",
        ),
        (
            "tablet",
            [800, 1280],
            "appearance",
            "25-tablet-upright-settings",
        ),
        (
            "tablet",
            [1280, 800],
            "display",
            "25a-tablet-on-its-side-settings",
        ),
        ("desktop", [720, 520], "display", "26-small-window-settings"),
    ] {
        tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text(interface.to_owned()));
        tour.camera.size = size;
        tour.request(ShellRequest::OpenSettings(Some(page.to_owned())));
        tour.shoot(name)?;
    }
    tour.runtime.shell.close_window();
    tour.set(sue_wm::INTERFACE_KEY, RiceValue::Text("auto".to_owned()));
    Ok(())
}

/// A small icon pack of filled shapes, unlike Sue's lines, so a sheet
/// shows at a glance which icons it draws.
fn write_test_pack(folder: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let manifest = "format = 1
name = Snapshot Shapes
author = Sue's snapshot tour
                    licence = CC0-1.0
version = 1
";
    let disc = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="8" fill="black"/></svg>"#;
    let square = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="5" y="5" width="14" height="14" rx="3" fill="black"/></svg>"#;
    let diamond = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 3 L21 12 L12 21 L3 12 Z" fill="black"/></svg>"#;
    let mut files = vec![("icon-pack.txt".to_owned(), manifest.to_owned())];
    for (index, name) in [
        "settings",
        "search",
        "folder",
        "delete",
        "info",
        "tool-brush",
        "picture",
        "document",
        "star",
        "heart",
        "eye",
        "lock",
        "close",
        "check",
        "plus",
        "minus",
        "menu",
        "filter",
    ]
    .iter()
    .enumerate()
    {
        let svg = [disc, square, diamond][index % 3];
        files.push((format!("{name}.svg"), svg.to_owned()));
    }
    for (name, text) in files {
        std::fs::write(folder.join(name), text).map_err(|error| error.to_string())?;
    }
    Ok(())
}
