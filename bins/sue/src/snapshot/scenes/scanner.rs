//! Scenes of 0.0.81's Hand Scanner and Packages: the scanner following
//! a hand, the scanner asking for its package, that package downloading,
//! and Settings, Packages at rest and in the middle of an install.
//!
//! No camera, no models and no network are used: the scanner runs on a
//! made-up camera whose picture shows a drawn hand, and the package
//! center on a fetcher that hands over made-up bytes up to a little
//! under half the package and then waits to be cancelled, so the
//! pictures are the same on every run.

use std::sync::Arc;
use std::time::Duration;

use egui::Rect;
use sue_paint::hand_scanner::{self, Parts, Script};
use sue_platform::packages::{
    Chunk, Fetch, FetchError, HAND_TRACKING, PackageCenter, PackageState,
};
use sue_wm::ShellRequest;

use super::super::Tour;

/// Hands over zeros until the package is a little under half there,
/// then waits for the install to be cancelled.
struct Stalling;

/// Where [`Stalling`] stops: bytes of the package's first file.
const STALL_AT: u64 = 3_600_000;

impl Fetch for Stalling {
    fn fetch(
        &self,
        _url: &str,
        _from: u64,
        sink: &mut dyn FnMut(Chunk<'_>) -> bool,
    ) -> Result<(), FetchError> {
        let piece = vec![0_u8; 64 * 1024];
        let mut sent = 0_u64;
        loop {
            if sent < STALL_AT {
                sent += piece.len() as u64;
                if !sink(Chunk::Data(&piece)) {
                    return Ok(());
                }
            } else {
                // Nothing more; an empty piece asks whether to go on.
                std::thread::sleep(Duration::from_millis(5));
                if !sink(Chunk::Data(&[])) {
                    return Ok(());
                }
            }
        }
    }
}

/// One frame; every piece of text it draws, with its rect.
fn texts(tour: &mut Tour<'_>) -> Vec<(String, Rect)> {
    let mut output = tour.camera.frame(&mut tour.runtime);
    // The camera's picture is a texture that changes every frame: its
    // changes are handed on, never dropped.
    tour.camera
        .painter
        .apply_textures(std::mem::take(&mut output.textures_delta));
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

/// Frames with real time between them, until `done` or five seconds:
/// the camera, the tracker and the download run on threads of their own.
fn until(
    tour: &mut Tour<'_>,
    what: &str,
    mut done: impl FnMut(&mut Tour<'_>) -> bool,
) -> Result<(), String> {
    let started = std::time::Instant::now();
    loop {
        let _ = texts(tour);
        if done(tour) {
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(5) {
            return Err(format!("the scene waited too long for {what}"));
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// Clicks the topmost `text` on screen.
fn click(tour: &mut Tour<'_>, text: &str) -> Result<(), String> {
    let at = texts(tour)
        .into_iter()
        .filter(|(found, _)| found == text)
        .map(|(_, rect)| rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .ok_or_else(|| format!("nothing on screen says {text:?}"))?
        .center();
    tour.camera.click(at);
    for _ in 0..8 {
        let _ = texts(tour);
    }
    Ok(())
}

/// A camera that shows a peace sign, which the tracker finds.
fn peace() -> Parts {
    let script: Script = Arc::new(|_| Some((sue_paint::figure::hand_preset(4), false)));
    Parts::scripted(Duration::from_millis(4), script)
}

/// 81a to 81e. The Hand Scanner and Packages.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    let root = tour.runtime.engine.catalog.folders["packages"].clone();
    let center = Arc::new(PackageCenter::new(&root, Arc::new(Stalling)));
    PackageCenter::put(Arc::clone(&center));
    let size = sue_wm::packages::human_bytes(
        center
            .package(HAND_TRACKING)
            .ok_or("Sue names no hand tracking package")?
            .size(),
    );
    let install = sue_lang::tr!("wm", "package-install", size = size);
    let cancel = sue_lang::text("wm", "package-cancel");
    let installing = |center: &PackageCenter| {
        matches!(
            center.state(HAND_TRACKING),
            PackageState::Installing { done, .. } if done >= STALL_AT
        )
    };

    // 81a. A 3D hand following the hand the camera shows.
    tour.paint_command("layer.new.hand");
    tour.settle();
    hand_scanner::inject(peace());
    tour.paint_command("figure.hand_scanner_camera");
    until(tour, "the hand to be followed", |_| {
        hand_scanner::state().is_some_and(|state| state.hands >= 12)
    })?;
    tour.shoot_entering("81a-hand-scanner", 60)?;
    click(tour, &sue_lang::text("paint", "cancel"))?;

    // 81b. Without its package the scanner asks for it first.
    let mut parts = peace();
    parts.needs_package = true;
    hand_scanner::inject(parts);
    tour.paint_command("figure.hand_scanner_camera");
    tour.shoot_entering("81b-hand-scanner-package", 60)?;
    // 81c. Install: the download's progress, its speed, and Cancel.
    click(tour, &install)?;
    until(tour, "the download", |_| installing(&center))?;
    tour.shoot_entering("81c-hand-scanner-installing", 2)?;
    click(tour, &cancel)?;
    until(tour, "the download to stop", |_| {
        center.state(HAND_TRACKING) == PackageState::Available
    })?;
    click(tour, &sue_lang::text("paint", "cancel"))?;
    tour.settle();
    // The hand's layer goes again.
    tour.paint_command("edit.undo");
    tour.settle();

    // 81d. Settings, Packages.
    tour.request(ShellRequest::OpenSettings(Some("packages".to_owned())));
    tour.shoot("81d-settings-packages")?;
    // 81e. The same page in the middle of an install.
    click(tour, &install)?;
    until(tour, "the download", |_| installing(&center))?;
    tour.shoot_entering("81e-settings-packages-installing", 2)?;
    click(tour, &cancel)?;
    until(tour, "the download to stop", |_| {
        center.state(HAND_TRACKING) == PackageState::Available
    })?;
    tour.runtime.shell.close_window();
    tour.settle();
    if std::fs::read_dir(&root).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err("a cancelled download left files behind".to_owned());
    }
    Ok(())
}
