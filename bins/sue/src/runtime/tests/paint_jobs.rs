//! Exports that run in the background: the window keeps drawing and
//! taking strokes while they run, they export the document as it was when
//! they began, Cancel leaves no file, and what they write is what an
//! export made at once writes. Also CTV-7, Export Timelapse with its
//! preview and its aspect ratio.

use super::paint_anim::{line_on_canvas, walk};
use super::*;
use std::path::Path;

/// Hands Sue Paint the file a save dialog for `purpose` would pick, and
/// leaves the export it starts running.
fn pick(paint: &mut PaintDriver, purpose: &str, path: &Path) {
    let applet = AppletId::new(sue_paint::ID).unwrap();
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(Some(applet), purpose, "Export", "x"),
            path: path.to_path_buf(),
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(2);
}

/// Runs `command`, takes what its settings dialog offers, and picks
/// `path` for it.
fn start_export(paint: &mut PaintDriver, command: &str, purpose: &str, path: &Path) {
    paint.command(command);
    paint.settle(3);
    paint.click_text("OK");
    pick(paint, purpose, path);
}

/// [`start_export`] made at once, the window waiting: the synchronous
/// export the background one is compared with.
fn export_now(paint: &mut PaintDriver, command: &str, purpose: &str, path: &Path) {
    sue_paint::export_at_once(true);
    start_export(paint, command, purpose, path);
    sue_paint::export_at_once(false);
    paint.settle(4);
}

/// One frame, the transfers read as the desktop's timer reads them; what
/// it drew.
fn frame(paint: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    paint.runtime.transfers.poll();
    paint.step(Vec::new())
}

/// The files in `dir` (not in its folders), by name.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn sue_paint_keeps_drawing_while_an_export_runs_and_exports_what_was_there() {
    let mut paint = PaintDriver::new("paint-jobs-live");
    walk(&mut paint);
    let root = paint.root.clone();
    // What the animation is now, made at once.
    let now = root.join("Now.gif");
    export_now(&mut paint, "file.export.gif", "anim-gif", &now);
    let reference = std::fs::read(&now).expect("the export made at once");
    // The same export in the background, held before it reads anything
    // of its snapshot, so whatever is drawn meanwhile is drawn before it
    // renders a frame.
    let gif = root.join("Walk.gif");
    sue_paint::hold_export_starts(&gif, true);
    start_export(&mut paint, "file.export.gif", "anim-gif", &gif);
    // The window draws every frame meanwhile, and each shows the
    // export's notice with its progress and Cancel.
    for _ in 0..30 {
        let texts = frame(&mut paint);
        assert!(
            texts.iter().any(|(text, _)| text == "Exporting Walk.gif"),
            "{texts:?}"
        );
        assert!(texts.iter().any(|(text, _)| text == "Cancel"));
    }
    assert!(!gif.exists(), "nothing written while it runs");
    let transfer = paint
        .runtime
        .transfers
        .list()
        .iter()
        .find(|transfer| transfer.path.as_deref() == Some(gif.as_path()))
        .map(|transfer| transfer.id)
        .expect("in the transfers list");
    assert!(
        paint.runtime.shell.working_notice_rects(transfer).is_some(),
        "its notice"
    );
    // The canvas takes a stroke while it runs: the document changes.
    let before = paint.runtime.engine.world.arena.revision();
    line_on_canvas(&mut paint, 0.0);
    assert_ne!(
        paint.runtime.engine.world.arena.revision(),
        before,
        "the stroke went in"
    );
    sue_paint::hold_export_starts(&gif, false);
    paint.finish_exports();
    // It exported the document as it was when it began, not the stroke
    // drawn meanwhile.
    assert_eq!(std::fs::read(&gif).unwrap(), reference);
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("Saved Walk.gif")),
        "{:?}",
        paint.texts
    );
    assert!(paint.rect_of("Show in Folder").is_some());
    assert!(paint.runtime.shell.working_notice_rects(transfer).is_none());
    // Listed once, done.
    let listed: Vec<_> = paint
        .runtime
        .transfers
        .list()
        .iter()
        .filter(|transfer| transfer.path.as_deref() == Some(gif.as_path()))
        .collect();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].state, sue_wm::TransferState::Done);
    // The stroke is in the animation: an export started now has it, so
    // the equal file above is the snapshot at work, not a stroke the
    // export could not have seen anyway.
    let after = root.join("After.gif");
    export_now(&mut paint, "file.export.gif", "anim-gif", &after);
    assert_ne!(
        std::fs::read(&after).unwrap(),
        reference,
        "the stroke shows"
    );
}

#[test]
fn sue_paint_cancelling_an_export_leaves_no_file() {
    let mut paint = PaintDriver::new("paint-jobs-cancel");
    walk(&mut paint);
    let root = paint.root.clone();
    let before = names_in(&root);
    sue_paint::hold_exports_to(&root, true);
    let mp4 = sue_platform::movie::mp4_available();
    let movie = root.join(if mp4 { "Walk.mp4" } else { "Walk.avi" });
    for (command, purpose, path) in [
        ("file.export.movie", "anim-movie", movie),
        (
            "file.export.sequence",
            "anim-sequence",
            root.join("Walk.png"),
        ),
        ("file.export.cels", "anim-cels", root.join("Cels.png")),
    ] {
        start_export(&mut paint, command, purpose, &path);
        frame(&mut paint);
        assert!(
            paint.rect_of("Cancel").is_some(),
            "{command}: {:?}",
            paint.texts
        );
        paint.click_text("Cancel");
        paint.finish_exports();
        assert!(
            paint
                .texts
                .iter()
                .any(|(text, _)| text.contains("nothing was written")),
            "{command}: {:?}",
            paint.texts
        );
        // No file, no temporary file and no folder of cels.
        assert_eq!(names_in(&root), before, "{command}");
        let last = &paint.runtime.transfers.list()[0];
        assert_eq!(last.state, sue_wm::TransferState::Cancelled, "{command}");
    }
    sue_paint::hold_exports_to(&root, false);
    // The same exports let run write their files.
    start_export(
        &mut paint,
        "file.export.sequence",
        "anim-sequence",
        &root.join("Walk.png"),
    );
    paint.finish_exports();
    assert!(names_in(&root).iter().any(|name| name.ends_with(".png")));
    assert!(
        !names_in(&root)
            .iter()
            .any(|name| name.ends_with(".sue-part")),
        "no temporary name left"
    );
}

/// The frames of the movie at `path`, decoded by FFmpeg to plain RGB, or
/// `None` where no FFmpeg runs (`SUE_FFMPEG` names one, else the one on
/// the path).
fn decoded(path: &Path) -> Option<Vec<u8>> {
    let ffmpeg = std::env::var("SUE_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_owned());
    let output = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .ok()?;
    assert!(
        output.status.success(),
        "FFmpeg reads {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    Some(output.stdout)
}

#[test]
fn sue_paint_a_movie_made_in_the_background_equals_one_made_at_once() {
    let mut paint = PaintDriver::new("paint-jobs-same");
    walk(&mut paint);
    let root = paint.root.clone();
    let mp4 = sue_platform::movie::mp4_available();
    let extension = if mp4 { "mp4" } else { "avi" };
    let at_once = root.join(format!("Now.{extension}"));
    export_now(&mut paint, "file.export.movie", "anim-movie", &at_once);
    let background = root.join(format!("Later.{extension}"));
    start_export(&mut paint, "file.export.movie", "anim-movie", &background);
    paint.finish_exports();
    let (a, b) = (
        std::fs::read(&at_once).expect("made at once"),
        std::fs::read(&background).expect("made in the background"),
    );
    if !mp4 {
        // Sue's own AVI writer: the very same bytes.
        assert_eq!(a, b);
        return;
    }
    assert_eq!(&b[4..8], b"ftyp");
    match (decoded(&at_once), decoded(&background)) {
        (Some(a), Some(b)) => {
            assert!(!a.is_empty(), "frames decoded");
            assert_eq!(a.len(), b.len(), "as many frames");
            assert!(a == b, "every frame the same");
            eprintln!(
                "FFmpeg decoded {} bytes of frames from each movie: equal",
                a.len()
            );
        }
        _ => eprintln!("no FFmpeg here: the movies were not decoded and compared"),
    }
}

#[test]
fn sue_paint_export_timelapse_previews_the_video_and_takes_its_shape() {
    // CTV-7: a preview that plays, the aspect ratio, and the progress of
    // the export in its notice.
    let mut paint = PaintDriver::new("paint-jobs-timelapse");
    paint.command("file.timelapse.record");
    paint.settle(3);
    for dy in [-40.0, 0.0, 40.0] {
        line_on_canvas(&mut paint, dy);
        paint.settle(20);
    }
    paint.command("file.timelapse.export");
    let start = Instant::now();
    // The preview's frames are read in the background.
    while !paint.texts.iter().any(|(text, _)| text.contains(" px, ")) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{:?}",
            paint.texts
        );
        paint.step(Vec::new());
    }
    assert!(paint.rect_of("Aspect ratio:").is_some());
    assert!(paint.rect_of("Same as the canvas").is_some());
    // It plays at once: the time moves on.
    let time = |paint: &PaintDriver| {
        paint
            .texts
            .iter()
            .find(|(text, _)| text.contains(" of "))
            .map(|(text, _)| text.clone())
            .unwrap_or_default()
    };
    let first = time(&paint);
    for _ in 0..90 {
        paint.step(Vec::new());
    }
    assert!(!first.is_empty(), "{:?}", paint.texts);
    assert_ne!(time(&paint), first, "it plays");
    // A square video: the size says so.
    paint.click_text("Same as the canvas");
    paint.click_text("1:1 (square)");
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("1,080 by 1,080 px")),
        "{:?}",
        paint.texts
    );
    paint.click_text("OK");
    paint.settle(2);
    let root = paint.root.clone();
    std::fs::create_dir_all(&root).unwrap();
    let mp4 = sue_platform::movie::mp4_available();
    let path = root.join(if mp4 { "Drawing.mp4" } else { "Drawing.avi" });
    pick(&mut paint, "timelapse", &path);
    paint.finish_exports();
    let bytes = std::fs::read(&path).expect("the timelapse is written");
    if mp4 {
        assert_eq!(&bytes[4..8], b"ftyp");
        if let Some(frames) = decoded(&path) {
            // Square, 1080 a side, three bytes a pixel.
            assert_eq!(frames.len() % (1080 * 1080 * 3), 0);
        }
    } else {
        assert_eq!(&bytes[..4], b"RIFF");
    }
}

