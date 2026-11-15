//! Files in Sue Paint: the export settings dialog before every export,
//! the formats Save Duplicate offers, Save As with Photoshop documents,
//! File, Batch Convert Pictures, File, Export Animation, Audio, and
//! File, Show in Folder. Each drives real frames and clicks what the
//! frames drew.

use super::*;

/// Every file under `dir` whose extension is `extension` (every file
/// for an empty one), sorted.
fn files_in(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if extension.is_empty()
                || path
                    .extension()
                    .is_some_and(|found| found.eq_ignore_ascii_case(extension))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The status line's text.
fn message(paint: &PaintDriver) -> String {
    paint
        .runtime
        .shell
        .message()
        .map(|(text, _)| text.to_string())
        .unwrap_or_default()
}

/// `true` when the last frame drew `text`.
fn shown(paint: &PaintDriver, text: &str) -> bool {
    paint.texts.iter().any(|(found, _)| found == text)
}

/// Opens the dropdown that shows `now` and picks `choice` from its list.
fn choose(paint: &mut PaintDriver, now: &str, choice: &str) {
    paint.click_text(now);
    paint.settle(4);
    // The list draws the choice below the closed field: the lowest one.
    let at = paint
        .texts
        .iter()
        .filter(|(found, _)| found == choice)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{choice:?} is in the list: {:?}", paint.texts));
    let press = |pressed| egui::Event::PointerButton {
        pos: at.center(),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at.center())]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(4);
}

/// `command` is grayed: run, it opens nothing, says nothing and
/// writes nothing.
fn grayed(paint: &mut PaintDriver, command: &str) {
    const MARK: &str = "(test: nothing said yet)";
    paint.runtime.shell.notify(MARK, false);
    let before = paint.settle(4);
    let files = files_in(&paint.root, "").len();
    paint.command(command);
    let after = paint.settle(6);
    assert_eq!(message(paint), MARK, "{command} said something");
    let texts = |texts: &[(String, egui::Rect)]| {
        texts
            .iter()
            .map(|(text, _)| text.clone())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(texts(&before), texts(&after), "{command} opened something");
    assert_eq!(files_in(&paint.root, "").len(), files, "{command} wrote");
}

/// Scrolls the dialog under `over` until `text` is drawn clear of its
/// footer.
fn scroll_to(paint: &mut PaintDriver, over: &str, text: &str) {
    let at = paint
        .rect_of(over)
        .unwrap_or_else(|| panic!("{over:?} is on screen"))
        .center();
    for _ in 0..40 {
        if paint
            .rect_of(text)
            .is_some_and(|rect| rect.bottom() < 780.0)
        {
            return;
        }
        paint.step(vec![
            egui::Event::PointerMoved(at),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -60.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        paint.settle(2);
    }
    panic!("{text:?} never came into view: {:?}", paint.texts);
}

/// Answers the file dialog the last command asked for with `path`.
fn answer(paint: &mut PaintDriver, path: &Path) {
    let now = paint.now;
    assert!(
        paint.runtime.pending_pick().is_some(),
        "a file dialog was asked for"
    );
    assert!(paint.runtime.answer_pick(path, &mut NoTextures, now));
    paint.settle(4);
}

/// A PNG of `width` by `height`: a red left half and a blue right one.
fn halves(width: u32, height: u32) -> Vec<u8> {
    let pixels = (0..width * height)
        .flat_map(|index| {
            if index % width < width / 2 {
                [220, 20, 20, 255]
            } else {
                [20, 20, 220, 255]
            }
        })
        .collect();
    let tile =
        sue_core::RasterTile2D::from_bytes(width, height, sue_core::PixelFormat::Rgba8, pixels)
            .unwrap();
    sue_io::encode_png(&tile).unwrap()
}

#[test]
fn every_flat_export_shows_the_export_settings_first() {
    // CT-18: Export (Single Layer) and Save Duplicate ask for the
    // settings before anything is written.
    let mut paint = PaintDriver::new("paint-export-settings");
    let pngs = files_in(&paint.root, "png").len();
    paint.command("file.export.png");
    paint.settle(6);
    assert!(shown(&paint, "Export Settings"), "{:?}", paint.texts);
    assert!(shown(&paint, "PNG picture"), "PNG is chosen");
    assert_eq!(files_in(&paint.root, "png").len(), pngs, "nothing yet");
    // Half the size, then Export.
    choose(&mut paint, "Keep the size", "Scale");
    assert!(shown(&paint, "Resampling"), "resizing offers the filters");
    assert!(shown(&paint, "Lanczos (sharpest)"), "{:?}", paint.texts);
    paint.click_text("Export");
    paint.settle(6);
    assert!(!shown(&paint, "Export Settings"), "the dialog closed");
    let written = files_in(&paint.root, "png");
    assert_eq!(written.len(), pngs + 1, "one PNG was written");
    let new = written
        .iter()
        .max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
        })
        .unwrap();
    let back = sue_io::decode_png(&std::fs::read(new).unwrap())
        .unwrap()
        .tile;
    let canvas = paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .unwrap();
    let size =
        sue_model::picture::size(paint.runtime.engine.world.arena.get_by_id(canvas).unwrap())
            .unwrap();
    assert_eq!(
        (back.width(), back.height()),
        (size[0].div_ceil(2), size[1].div_ceil(2)),
        "written at the 50 % the dialog asked for"
    );
    // The next export starts where this one left the settings; Cancel
    // writes nothing.
    paint.command("file.export.jpg");
    paint.settle(6);
    assert!(shown(&paint, "JPEG picture"), "the command's format");
    assert!(shown(&paint, "Scale"), "the size kept from before");
    assert!(shown(&paint, "Quality"), "JPEG asks for its quality");
    paint.click_text("Cancel");
    paint.settle(4);
    assert!(
        files_in(&paint.root, "jpg").is_empty(),
        "Cancel wrote nothing"
    );
}

#[test]
fn save_duplicate_writes_gif_qoi_ico_and_ppm() {
    // FGV-44: Save Duplicate offers more than .clip, these four too.
    let mut paint = PaintDriver::new("paint-export-duplicate");
    for (command, extension, format) in [
        ("file.duplicate.gif", "gif", sue_io::PictureFormat::Gif),
        ("file.duplicate.qoi", "qoi", sue_io::PictureFormat::Qoi),
        ("file.duplicate.ico", "ico", sue_io::PictureFormat::Ico),
        ("file.duplicate.ppm", "ppm", sue_io::PictureFormat::Pnm),
    ] {
        paint.command(command);
        paint.settle(6);
        assert!(shown(&paint, "Export Settings"), "{command}");
        if extension == "ico" {
            assert!(
                paint
                    .texts
                    .iter()
                    .any(|(text, _)| text.contains("at most") && text.contains("256")),
                "an icon says it is fitted: {:?}",
                paint.texts
            );
        }
        paint.click_text("Export");
        paint.settle(6);
        let written = files_in(&paint.root, extension);
        assert_eq!(written.len(), 1, "{command} wrote one .{extension}");
        let bytes = std::fs::read(&written[0]).unwrap();
        let back = sue_io::decode_picture(&bytes, Some(format)).unwrap();
        assert!(back.width() > 0 && back.height() > 0, "{command}");
        if extension == "ico" {
            assert!(back.width().max(back.height()) <= 256, "fitted");
        }
    }
}

#[test]
fn save_as_writes_a_layered_photoshop_document() {
    // CC-10: Save As takes .psd and .psb, with the layers.
    let mut paint = PaintDriver::new("paint-export-save-as-psd");
    paint.command("layer.new");
    paint.settle(4);
    let paint_id = AppletId::new(sue_paint::ID).unwrap();
    for (name, big) in [("layers.psd", false), ("layers.psb", true)] {
        let path = paint.root.join(name);
        let now = paint.now;
        paint.runtime.request(
            ShellRequest::Picked {
                pick: sue_wm::FilePick::save(Some(paint_id.clone()), "save-as", "Save As", name),
                path: path.clone(),
            },
            &mut NoTextures,
            now,
        );
        paint.settle(4);
        let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        // The version field: 1 for a document, 2 for a big one.
        assert_eq!(bytes[5], if big { 2 } else { 1 }, "{name}");
        let psd = sue_io::psd::decode_psd(&bytes).unwrap();
        assert!(psd.layers.len() >= 2, "{name} kept the layers");
    }
}

#[test]
fn a_folder_of_three_pngs_becomes_three_turned_jpegs_beside_them() {
    let mut paint = PaintDriver::new("paint-export-batch");
    let folder = paint.root.join("scans");
    std::fs::create_dir_all(&folder).unwrap();
    for name in ["a.png", "b.png", "c.png"] {
        std::fs::write(folder.join(name), halves(8, 4)).unwrap();
    }
    std::fs::write(folder.join("notes.txt"), "not a picture").unwrap();
    paint.command("file.batch_convert");
    // The dialog rises in: let it come to rest before clicking.
    paint.settle(40);
    assert!(shown(&paint, "Batch Convert Pictures"), "{:?}", paint.texts);
    assert!(shown(&paint, "Change every picture"), "{:?}", paint.texts);
    paint.click_text("Add a Folder");
    answer(&mut paint, &folder);
    assert!(shown(&paint, "Convert 3 Pictures"), "{:?}", paint.texts);
    // Into JPEG, turned a quarter to the right.
    choose(&mut paint, "PNG picture", "JPEG picture");
    scroll_to(&mut paint, "Convert to", "90° right");
    paint.click_text("90° right");
    paint.click_text("Convert 3 Pictures");
    for _ in 0..400 {
        if files_in(&folder, "jpg").len() == 3 {
            let done = paint
                .texts
                .iter()
                .filter(|(text, _)| text.starts_with("Done: "))
                .count();
            if done == 3 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
        paint.settle(1);
    }
    let jpegs = files_in(&folder, "jpg");
    let names: Vec<String> = jpegs
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["a.jpg", "b.jpg", "c.jpg"], "beside their files");
    for jpeg in &jpegs {
        let back = sue_io::decode_jpeg(&std::fs::read(jpeg).unwrap())
            .unwrap()
            .tile;
        assert_eq!((back.width(), back.height()), (4, 8), "turned: {jpeg:?}");
        // The red half was on the left; turned right it is on top.
        let pixels = back.bytes().unwrap();
        let top = &pixels[..4];
        let bottom = &pixels[pixels.len() - 4..];
        assert!(top[0] > 150 && top[2] < 100, "red on top: {top:?}");
        assert!(bottom[2] > 150 && bottom[0] < 100, "blue below: {bottom:?}");
    }
    // The originals stay, and the run said where the first one went.
    assert_eq!(files_in(&folder, "png").len(), 3);
    assert!(message(&paint).contains("a.jpg"), "{}", message(&paint));
    // Closed, the dialog takes the keys no more.
    paint.click_text("Close");
    paint.settle(4);
    assert!(!shown(&paint, "Batch Convert Pictures"));
}

#[test]
fn a_timeline_with_one_sound_exports_a_24_bit_flac_at_48000_hz() {
    let mut paint = PaintDriver::new("paint-export-audio");
    // ABV-15: without sound the command is grayed: it does nothing.
    grayed(&mut paint, "file.export.audio");
    paint.new_timeline();
    paint.settle(6);
    let samples: Vec<f32> = (0..16_000)
        .map(|at| (at as f32 * 0.05).sin() * 0.5)
        .collect();
    let pcm = sue_core::PCMFloat1D::new(16_000, 1, samples).unwrap();
    let sound = paint.root.join("beep.wav");
    std::fs::write(&sound, sue_io::encode_wav(&pcm)).unwrap();
    let now = paint.now;
    paint
        .runtime
        .request(ShellRequest::OpenFile(sound), &mut NoTextures, now);
    paint.settle(6);
    paint.command("file.export.audio");
    paint.settle(6);
    assert!(shown(&paint, "Export Audio"), "{:?}", paint.texts);
    choose(&mut paint, "WAV sound", "FLAC sound");
    choose(&mut paint, "16-bit (CD)", "24-bit (studio)");
    choose(&mut paint, "Keep the rate", "48,000 Hz");
    paint.click_text("Export");
    paint.settle(6);
    let flacs = files_in(&paint.root, "flac");
    assert_eq!(flacs.len(), 1, "one FLAC was written");
    let bytes = std::fs::read(&flacs[0]).unwrap();
    assert_eq!(&bytes[..4], b"fLaC");
    // STREAMINFO: the sample rate (20 bits) and bits per sample less one
    // (5 bits), from its eleventh byte on.
    let info = &bytes[8..];
    let rate =
        (u32::from(info[10]) << 12) | (u32::from(info[11]) << 4) | (u32::from(info[12]) >> 4);
    let bits = (((info[12] & 1) << 4) | (info[13] >> 4)) + 1;
    assert_eq!(rate, 48_000);
    assert_eq!(bits, 24);
    let probed = sue_io::probe_audio(&flacs[0]).unwrap();
    assert_eq!(probed.sample_rate, 48_000);
    // One second of sound, mixed at the timeline's rate.
    assert!(
        (probed.frames as i64 - 48_000).abs() < 200,
        "{} frames",
        probed.frames
    );
}

#[test]
fn show_in_folder_shows_the_canvas_file() {
    let mut paint = PaintDriver::new("paint-export-reveal");
    // A new canvas has no file yet: the command is grayed.
    grayed(&mut paint, "file.show_in_folder");
    // Opened from a picture, it shows that picture.
    let picture = paint.root.join("photo.png");
    std::fs::write(&picture, halves(6, 4)).unwrap();
    let now = paint.now;
    paint.runtime.open_path(&picture, &mut NoTextures, now);
    paint.settle(6);
    paint.command("file.show_in_folder");
    paint.settle(4);
    assert_eq!(
        message(&paint),
        format!("File: {}", picture.display()),
        "the file manager is asked to show it"
    );
    // Exported, the canvas's file is the one written last.
    paint.command("file.export.webp");
    paint.settle(6);
    paint.click_text("Export");
    paint.settle(6);
    let webp = files_in(&paint.root, "webp");
    assert_eq!(webp.len(), 1);
    paint.command("file.show_in_folder");
    paint.settle(4);
    assert_eq!(message(&paint), format!("File: {}", webp[0].display()));
}

#[test]
fn the_files_dialogs_keep_the_pen_and_the_keys_off_the_canvas() {
    // While the export settings or Batch Convert Pictures show, a drag
    // over them paints nothing beneath and Delete clears nothing.
    let mut paint = PaintDriver::new("paint-export-modal");
    let paint_id = AppletId::new(sue_paint::ID).unwrap();
    paint.command("tool.pen");
    paint.settle(4);
    let middle = paint_middle(&paint.runtime);
    let depth = paint.runtime.engine.history.depth();
    paint.drag(
        middle + vec2(-40.0, -20.0),
        middle + vec2(40.0, 20.0),
        egui::PointerButton::Primary,
    );
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "the pen paints with no dialog up"
    );
    for command in ["file.export.png", "file.batch_convert"] {
        paint.command(command);
        paint.settle(40);
        let depth = paint.runtime.engine.history.depth();
        paint.drag(
            middle + vec2(-60.0, 10.0),
            middle + vec2(60.0, 30.0),
            egui::PointerButton::Primary,
        );
        paint.step(vec![key(egui::Key::Delete, egui::Modifiers::NONE)]);
        paint.settle(4);
        assert_eq!(
            paint.runtime.engine.history.depth(),
            depth,
            "{command}: nothing changed under the dialog"
        );
        assert!(
            paint
                .runtime
                .applets
                .get_mut(&paint_id)
                .is_some_and(|applet| applet.navigate("dialog/close"))
        );
        paint.settle(6);
    }
}
