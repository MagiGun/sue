//! 0.0.81: the Hand Scanner and the Packages section, driven through the
//! real runtime with a made-up camera, a made-up tracker and a fetcher
//! that needs no network, so they run on any machine: the scanner opens,
//! pictures arrive, the hand's pose follows, OK is one undo step, Cancel
//! puts the hand back, a missing package shows Install, an install runs
//! and starts the scanner, and a cancelled download leaves nothing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use sue_paint::figure::{Body, HAND, Kind, Pose, hand_preset};
use sue_paint::hand_scan::{self, Side};
use sue_paint::hand_scanner::{self, Parts, Script};
use sue_pkg::ContentHash;
use sue_platform::packages::{
    Chunk, Fetch, FetchError, HAND_TRACKING, PackageCenter, PackageState,
};

use super::*;

/// The whole arena as bytes, so two states compare exactly.
fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    paint.runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

/// The picture of the canvas Sue Paint has open.
fn picture(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The pose and the body of the figure the canvas's 3D layer holds.
fn figure(paint: &PaintDriver) -> (Pose, Body) {
    let arena = &paint.runtime.engine.world.arena;
    sue_paint::doc::layers(arena, picture(paint))
        .iter()
        .find_map(|info| sue_paint::figure::of_layer(arena, info.id))
        .and_then(|model| arena.get_by_id(model).ok())
        .map(|model| (Pose::of(model), Body::of(model)))
        .expect("a figure is on the canvas")
}

/// Frames, with real time between them (the camera and the tracker run
/// on threads of their own), until `done` or ten seconds.
fn until(paint: &mut PaintDriver, what: &str, mut done: impl FnMut(&mut PaintDriver) -> bool) {
    let started = std::time::Instant::now();
    loop {
        paint.settle(1);
        if done(paint) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "waited too long for {what}; on screen: {:?}",
            paint
                .texts
                .iter()
                .map(|(text, _)| text.as_str())
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn shows(paint: &PaintDriver, text: &str) -> bool {
    paint.texts.iter().any(|(found, _)| found.contains(text))
}

/// Opens the Hand Scanner and lets its dialog come in, so what is
/// clicked next stands where it was found.
fn open_scanner(paint: &mut PaintDriver) {
    paint.command("figure.hand_scanner_camera");
    paint.settle(12);
}

/// A camera that always shows the hand `preset`, a left or a right one.
fn showing(preset: usize, right: bool) -> Parts {
    let script: Script = Arc::new(move |_| Some((hand_preset(preset), right)));
    Parts::scripted(Duration::from_millis(2), script)
}

/// The hand on the canvas has every finger curled.
fn is_fist(pose: &Pose) -> bool {
    [1, 4, 7, 10].iter().all(|first| pose.0[*first][0] > 75.0)
}

#[test]
fn the_hand_scanner_poses_a_3d_hand_live_and_ok_is_one_undo_step() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-scanner-ok");
    paint.command("layer.new.hand");
    paint.settle(3);
    let (open, body) = figure(&paint);
    assert_eq!(body.kind, Kind::Hand);
    assert!(!is_fist(&open));
    let before = arena_bytes(&paint);
    let depth = paint.runtime.engine.history.depth();

    assert_eq!(hand_scanner::state(), None);
    hand_scanner::inject(showing(2, false));
    open_scanner(&mut paint);
    until(&mut paint, "the scanner to open", |_| {
        hand_scanner::state().is_some_and(|state| state.camera)
    });
    // What the dialog says: its name, the camera, Pause Capture, the
    // palm hint and the line on privacy, and OK and Cancel.
    for text in [
        "Hand Scanner",
        "Camera",
        "Demo camera",
        "Pause Capture",
        "Turn the whole hand too",
        "palm facing the camera",
        "never saved and never sent anywhere",
        "OK",
        "Cancel",
    ] {
        assert!(shows(&paint, text), "{text:?} is in the dialog");
    }
    // Pictures arrive, a hand is found in them, and the hand on the
    // canvas takes its pose.
    until(&mut paint, "the hand to follow", |paint| {
        is_fist(&figure(paint).0)
    });
    let state = hand_scanner::state().expect("open");
    assert!(state.pictures >= 1 && state.hands >= 1, "{state:?}");
    assert!(shows(&paint, "Following a left hand"));
    // Following is no step in the history: the pose is only shown.
    assert_eq!(paint.runtime.engine.history.depth(), depth);

    paint.click_text("OK");
    assert_eq!(
        hand_scanner::state(),
        None,
        "the camera stops with its dialog"
    );
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "OK is one undo step"
    );
    let kept = figure(&paint).0;
    assert!(is_fist(&kept));
    // The fist of the preset, to a few degrees (the pose is smoothed).
    let want = hand_preset(2);
    for (at, joint) in HAND.iter().enumerate().take(13).skip(1) {
        assert!(
            (kept.0[at][0] - want.0[at][0]).abs() < 4.0,
            "{}: {} is not {}",
            joint.name,
            kept.0[at][0],
            want.0[at][0]
        );
    }
    let after = arena_bytes(&paint);
    paint.command("edit.undo");
    paint.settle(3);
    assert!(
        arena_bytes(&paint) == before,
        "undo puts the hand back exactly"
    );
    paint.command("edit.redo");
    paint.settle(3);
    assert!(arena_bytes(&paint) == after, "redo poses it again");
}

#[test]
fn cancel_puts_the_hand_back_and_pause_holds_it_still() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-scanner-cancel");
    paint.command("layer.new.hand");
    paint.settle(3);
    let before = arena_bytes(&paint);
    let depth = paint.runtime.engine.history.depth();

    // A camera that shows a fist, then an open hand, then a fist, each
    // for a while; and a right hand, which poses the same.
    let script: Script =
        Arc::new(|n| Some((hand_preset(if (n / 40) % 2 == 0 { 2 } else { 0 }), true)));
    hand_scanner::inject(Parts::scripted(Duration::from_millis(2), script));
    open_scanner(&mut paint);
    until(&mut paint, "the hand to follow", |paint| {
        is_fist(&figure(paint).0)
    });
    assert!(shows(&paint, "Following a right hand"));

    // Paused: the camera's picture and the pose stand still.
    paint.click_text("Pause Capture");
    assert!(hand_scanner::state().is_some_and(|state| state.paused));
    assert!(shows(&paint, "Resume Capture") && shows(&paint, "Capture is paused"));
    // What was on its way when it paused arrives; then nothing moves.
    for _ in 0..10 {
        paint.settle(1);
        std::thread::sleep(Duration::from_millis(5));
    }
    let held = (
        figure(&paint).0,
        hand_scanner::state().expect("open").pictures,
    );
    for _ in 0..40 {
        paint.settle(1);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(figure(&paint).0, held.0, "the pose moved while paused");
    assert_eq!(hand_scanner::state().expect("open").pictures, held.1);
    // Resumed: it follows again, through the open hand and back.
    paint.click_text("Resume Capture");
    until(&mut paint, "more pictures", |_| {
        hand_scanner::state().is_some_and(|state| state.pictures > held.1 + 5)
    });

    paint.click_text("Cancel");
    assert_eq!(hand_scanner::state(), None);
    assert!(
        arena_bytes(&paint) == before,
        "Cancel puts the hand back exactly"
    );
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth,
        "and is no step"
    );
}

#[test]
fn only_a_hand_or_a_figure_can_be_scanned_and_a_figure_takes_one_hand() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-scanner-figure");
    // On a raster layer the command is off.
    hand_scanner::inject(showing(2, false));
    open_scanner(&mut paint);
    paint.settle(3);
    assert_eq!(hand_scanner::state(), None);
    assert!(!shows(&paint, "Pause Capture"));

    // A whole drawing figure.
    paint.command("layer.new.figure");
    paint.settle(3);
    let (stood, body) = figure(&paint);
    assert_eq!(body.kind, Kind::Body);
    let before = arena_bytes(&paint);
    open_scanner(&mut paint);
    paint.settle(3);
    let Some(right) = hand_scan::body_fingers(&body.joints(), Side::Right) else {
        // A figure whose hands have no finger joints (before 0.0.81's
        // figures) has nothing to pose: the scanner says so and stays
        // shut.
        assert_eq!(hand_scanner::state(), None);
        assert_eq!(
            paint.runtime.shell.message().map(|(text, _)| text),
            Some("This drawing figure has no finger joints to pose")
        );
        assert!(arena_bytes(&paint) == before);
        return;
    };
    let left = hand_scan::body_fingers(&body.joints(), Side::Left).expect("both hands");
    // The dialog asks which hand, and poses the right one first.
    until(&mut paint, "the right hand to follow", |paint| {
        figure(paint).0.0[right[0]][0] > 75.0
    });
    assert!(shows(&paint, "Left hand") && shows(&paint, "Right hand"));
    let posed = figure(&paint).0;
    for at in left {
        assert_eq!(posed.0[at], stood.0[at], "the left hand is left alone");
    }
    // Nothing but the right hand's fingers moved.
    for (at, turn) in posed.0.iter().enumerate() {
        if !right.contains(&at) {
            assert_eq!(*turn, stood.0[at], "joint {at} moved");
        }
    }
    // The other hand: the right one goes back, the left one follows.
    paint.click_text("Left hand");
    until(&mut paint, "the left hand to follow", |paint| {
        figure(paint).0.0[left[0]][0] > 75.0
    });
    let posed = figure(&paint).0;
    for at in right {
        assert_eq!(posed.0[at], stood.0[at], "the right hand went back");
    }
    paint.click_text("Cancel");
    assert!(arena_bytes(&paint) == before);
}

/// A camera that fails says why in the dialog, in its own words, and
/// Try Again starts it anew.
#[test]
fn a_camera_that_cannot_be_used_says_why() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-scanner-camera");
    paint.command("layer.new.hand");
    paint.settle(3);
    let before = arena_bytes(&paint);
    let tries = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&tries);
    let mut parts = showing(2, false);
    let working = Arc::clone(&parts.camera);
    // Busy the first time it is opened, fine the second.
    parts.camera = Arc::new(move |id| {
        if count.fetch_add(1, Ordering::Relaxed) == 0 {
            sue_platform::camera::FakeCamera::failing(sue_platform::camera::CameraError::Busy)
        } else {
            working(id)
        }
    });
    hand_scanner::inject(parts);
    open_scanner(&mut paint);
    until(&mut paint, "the camera's answer", |paint| {
        shows(paint, "Another program is using the camera")
    });
    assert!(shows(&paint, "Try Again"));
    assert!(arena_bytes(&paint) == before, "nothing was posed");
    paint.click_text("Try Again");
    until(&mut paint, "the hand to follow", |paint| {
        is_fist(&figure(paint).0)
    });
    assert!(!shows(&paint, "Another program is using the camera"));
    assert_eq!(tries.load(Ordering::Relaxed), 2);
    paint.click_text("Cancel");
    assert!(arena_bytes(&paint) == before);
}

/// Serves the made-up package's files from memory, a piece at a time;
/// while `slow` it waits between pieces, so a test can cancel in the
/// middle of a download.
struct Served {
    files: Vec<(String, Vec<u8>)>,
    slow: AtomicBool,
    offline: AtomicBool,
    pieces: AtomicUsize,
}

impl Fetch for Served {
    fn fetch(
        &self,
        url: &str,
        from: u64,
        sink: &mut dyn FnMut(Chunk<'_>) -> bool,
    ) -> Result<(), FetchError> {
        if self.offline.load(Ordering::Relaxed) {
            return Err(FetchError::Offline("no network".to_owned()));
        }
        let (_, content) = self
            .files
            .iter()
            .find(|(name, _)| name == url)
            .ok_or_else(|| FetchError::Failed(format!("{url}: 404")))?;
        for piece in content[from as usize..].chunks(1024) {
            self.pieces.fetch_add(1, Ordering::Relaxed);
            if !sink(Chunk::Data(piece)) {
                return Ok(());
            }
            if self.slow.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(3));
            }
        }
        Ok(())
    }
}

/// The packages folder the runtime runs with, and a center put over it
/// that names one package, Hand tracking, with two small made-up files
/// served from memory.
fn fake_center(paint: &PaintDriver) -> (std::path::PathBuf, Arc<PackageCenter>, Arc<Served>) {
    let root = paint.runtime.engine.catalog.folders["packages"].clone();
    let files: Vec<(String, Vec<u8>)> = [("palm_detection.onnx", 1_u8), ("hand_landmark.onnx", 2)]
        .into_iter()
        .map(|(name, seed)| {
            let bytes = (0..60_000_u32)
                .map(|at| (at as u8).wrapping_mul(7).wrapping_add(seed))
                .collect();
            (format!("https://packages.test/{name}"), bytes)
        })
        .collect();
    let mut manifest = "format = 1\nid = hand-tracking\nname = Hand tracking\nversion = 1.0.0\n\
        kind = ai-model\nsummary = Finds your hand in the camera's picture.\n\
        licence = Apache-2.0\nauthor = A test\nhomepage = https://packages.test\n"
        .to_owned();
    for (url, bytes) in &files {
        manifest.push_str(&format!(
            "file = {} | {} | {} | {url}\n",
            url.rsplit('/').next().unwrap(),
            bytes.len(),
            ContentHash::of(bytes).to_hex()
        ));
    }
    let served = Arc::new(Served {
        files,
        slow: AtomicBool::new(false),
        offline: AtomicBool::new(false),
        pieces: AtomicUsize::new(0),
    });
    let fetch: Arc<dyn Fetch> = served.clone();
    let center = Arc::new(PackageCenter::with_packages(&root, fetch, &[&manifest]));
    PackageCenter::put(Arc::clone(&center));
    (root, center, served)
}

/// Everything under `folder`, as paths from it.
fn tree(folder: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut walk = vec![folder.to_path_buf()];
    while let Some(next) = walk.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            out.push(
                path.strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            if path.is_dir() {
                walk.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_missing_package_shows_install_and_a_cancelled_download_leaves_nothing() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-scanner-package");
    let (root, center, served) = fake_center(&paint);
    paint.command("layer.new.hand");
    paint.settle(3);
    let before = arena_bytes(&paint);

    // With the real models' place empty, the scanner asks for the
    // package first: what it is, its size, its licence, and Install. The
    // camera stays off.
    let mut parts = showing(2, false);
    parts.needs_package = true;
    hand_scanner::inject(parts);
    open_scanner(&mut paint);
    paint.settle(4);
    for text in [
        "Hand Scanner",
        "needs the Hand tracking package",
        "Hand tracking",
        "Not installed",
        "117.2 KiB",
        "licence Apache-2.0",
        "Install (117.2 KiB)",
        "never saved and never sent anywhere",
    ] {
        assert!(shows(&paint, text), "{text:?} is in the dialog");
    }
    assert_eq!(
        hand_scanner::state().map(|state| state.camera),
        Some(false),
        "the camera waits for the package"
    );
    assert!(!shows(&paint, "Pause Capture"));

    // Install, slowly, and cancel in the middle: the download stops and
    // nothing of it is left.
    served.slow.store(true, Ordering::Relaxed);
    paint.click_text("Install (117.2 KiB)");
    until(&mut paint, "the download to get going", |_| {
        matches!(
            center.state(HAND_TRACKING),
            PackageState::Installing { done, .. } if done > 4096
        )
    });
    assert!(shows(&paint, "Installing"));
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.contains(" of 117.2 KiB")),
        "the progress is told: {:?}",
        paint.texts
    );
    // The card's Cancel is the upper one; the dialog's own is under it.
    paint.click_text("Cancel");
    until(&mut paint, "the download to stop", |_| {
        center.state(HAND_TRACKING) == PackageState::Available
    });
    let pieces = served.pieces.load(Ordering::Relaxed);
    assert!(pieces < 118, "the download went on to its end");
    assert!(tree(&root).is_empty(), "left behind: {:?}", tree(&root));
    assert!(hand_scanner::state().is_some(), "the dialog is still open");
    assert!(shows(&paint, "Install (117.2 KiB)"));
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        served.pieces.load(Ordering::Relaxed),
        pieces,
        "it kept fetching"
    );

    // With no network the install says so, and can be tried again.
    served.offline.store(true, Ordering::Relaxed);
    paint.click_text("Install (117.2 KiB)");
    until(&mut paint, "the failure", |paint| {
        shows(paint, "could not reach the server")
    });
    served.offline.store(false, Ordering::Relaxed);
    served.slow.store(false, Ordering::Relaxed);
    paint.click_text("Try Again");
    // Installed: the scanner starts by itself and the hand follows.
    until(&mut paint, "the hand to follow", |paint| {
        is_fist(&figure(paint).0)
    });
    assert_eq!(center.state(HAND_TRACKING), PackageState::Installed);
    assert_eq!(
        tree(&root),
        [
            "hand-tracking",
            "hand-tracking/hand_landmark.onnx",
            "hand-tracking/package.suepkg",
            "hand-tracking/palm_detection.onnx",
        ]
    );
    assert!(shows(&paint, "Pause Capture"));
    paint.click_text("Cancel");
    assert_eq!(hand_scanner::state(), None);
    assert!(arena_bytes(&paint) == before);
}

/// Opens Settings at the page `id`.
fn open_settings(paint: &mut PaintDriver, id: &str) {
    paint.runtime.request(
        ShellRequest::OpenSettings(Some(id.to_owned())),
        &mut NoTextures,
        paint.now,
    );
    paint.settle(8);
}

#[test]
fn settings_packages_installs_cancels_and_uninstalls() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-packages");
    let (root, center, served) = fake_center(&paint);
    open_settings(&mut paint, "packages");
    for text in [
        "Packages",
        "OPTIONAL DOWNLOADS",
        "Hand tracking",
        "Finds your hand in the camera's picture.",
        "Not installed",
        "117.2 KiB, version 1.0.0, licence Apache-2.0",
        "From: A test",
        "Install (117.2 KiB)",
        "Install from a File",
    ] {
        assert!(
            shows(&paint, text),
            "{text:?} is on the page: {:?}",
            paint.texts
        );
    }

    // Cancelled in the middle: nothing is left.
    served.slow.store(true, Ordering::Relaxed);
    paint.click_text("Install (117.2 KiB)");
    until(&mut paint, "the download to get going", |paint| {
        shows(paint, " of 117.2 KiB")
            && matches!(center.state(HAND_TRACKING), PackageState::Installing { done, .. } if done > 2048)
    });
    paint.click_text("Cancel");
    until(&mut paint, "the download to stop", |paint| {
        shows(paint, "Install (117.2 KiB)")
    });
    assert_eq!(center.state(HAND_TRACKING), PackageState::Available);
    assert!(tree(&root).is_empty(), "left behind: {:?}", tree(&root));

    // Installed, then uninstalled again.
    served.slow.store(false, Ordering::Relaxed);
    paint.click_text("Install (117.2 KiB)");
    until(&mut paint, "the install", |paint| shows(paint, "Uninstall"));
    assert!(shows(&paint, "Installed"));
    assert_eq!(center.state(HAND_TRACKING), PackageState::Installed);
    assert!(center.folder(HAND_TRACKING).is_some());
    paint.click_text("Uninstall");
    until(&mut paint, "the removal", |paint| {
        shows(paint, "Install (117.2 KiB)")
    });
    assert!(tree(&root).is_empty(), "left behind: {:?}", tree(&root));
    assert!(
        center.package(HAND_TRACKING).is_some(),
        "it can be installed again"
    );

    // A package someone shared: a manifest with its file next to it.
    let shared = paint.root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let brushes = vec![7_u8; 3000];
    std::fs::write(shared.join("inks.bin"), &brushes).unwrap();
    let manifest = shared.join("inks.suepkg");
    std::fs::write(
        &manifest,
        format!(
            "format = 1\nid = shared-inks\nname = Shared inks\nversion = 0.3.0\nkind = codec\n\
             summary = Inks a friend made.\nlicence = CC0-1.0\nauthor = A friend\n\
             file = inks.bin | 3000 | {} | inks.bin\n",
            ContentHash::of(&brushes).to_hex()
        ),
    )
    .unwrap();
    paint.click_text("Install from a File");
    assert_eq!(
        paint
            .runtime
            .pending_pick()
            .map(|pick| pick.purpose.as_str()),
        Some("package-manifest")
    );
    let now = paint.now;
    assert!(paint.runtime.answer_pick(&manifest, &mut NoTextures, now));
    until(&mut paint, "the shared package", |_| {
        center.state("shared-inks") == PackageState::Installed
    });
    paint.settle(3);
    for text in ["Shared inks", "Inks a friend made.", "licence CC0-1.0"] {
        assert!(shows(&paint, text), "{text:?} is on the page");
    }
    assert_eq!(
        std::fs::read(root.join("shared-inks").join("inks.bin")).unwrap(),
        brushes
    );
    // A file that is no manifest is refused, in words.
    std::fs::write(&manifest, "not a package").unwrap();
    paint.click_text("Install from a File");
    let now = paint.now;
    assert!(paint.runtime.answer_pick(&manifest, &mut NoTextures, now));
    paint.settle(3);
    assert!(shows(&paint, "That file could not be used as a package"));
    paint.runtime.shell.close_window();
}
