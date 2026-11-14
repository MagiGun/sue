//! Saving and keeping work (P3-9): a save runs on a job while people go
//! on working, a failed save leaves the old file whole, every save writes
//! a preview, unsaved work goes onto a journal, and the work Sue 0.0.75
//! kept is still offered.

use super::*;
use sue_core::file::THUMBNAIL;
use sue_core::{ComponentData, ComponentKind};

/// A runtime with a Square Post picture open in Sue Paint.
fn with_picture(tag: &str, remembering: bool) -> (Runtime, PathBuf) {
    let (runtime, root) = runtime(tag);
    let mut runtime = runtime.remembering(remembering);
    runtime.request(
        ShellRequest::Create {
            applet: AppletId::new(sue_paint::ID).unwrap(),
            key: "Square Post".to_owned(),
        },
        &mut NoTextures,
        Instant::now(),
    );
    std::fs::create_dir_all(&root).unwrap();
    (runtime, root)
}

/// The first layer with pixels, by handle.
fn first_raster(runtime: &Runtime) -> sue_core::AssetHandle {
    first_raster_in(&runtime.engine.world.arena)
}

/// The first layer with pixels in `arena`, by handle.
fn first_raster_in(arena: &sue_core::Arena) -> sue_core::AssetHandle {
    arena
        .iter()
        .find(|(_, entity)| {
            entity.class() == "sue.layer" && entity.component(ComponentKind::RasterTile2D).is_some()
        })
        .map(|(handle, _)| handle)
        .expect("a raster layer")
}

/// Paints a small square into the first layer, in place, the way a patch
/// does.
fn paint(runtime: &mut Runtime, at: u32, shade: u8) {
    let handle = first_raster(runtime);
    let entity = runtime.engine.world.arena.get_mut(handle).unwrap();
    let Some(ComponentData::RasterTile2D(raster)) =
        entity.component_mut(ComponentKind::RasterTile2D)
    else {
        panic!("no pixels");
    };
    let stride = raster.width() as usize * 4;
    let bytes = raster.bytes_mut_in([at, at, 8, 8]).unwrap();
    for row in at..at + 8 {
        for column in at..at + 8 {
            let index = row as usize * stride + column as usize * 4;
            bytes[index..index + 4].copy_from_slice(&[shade, 0, 0, 255]);
        }
    }
}

/// A writer that fails after `left` bytes.
struct Cut<'a> {
    out: &'a mut Vec<u8>,
    left: u64,
}

impl std::io::Write for Cut<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            return Err(std::io::Error::other("cut"));
        }
        let take = bytes.len().min(self.left as usize);
        self.out.extend_from_slice(&bytes[..take]);
        self.left -= take as u64;
        Ok(take)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What a save of `arena` into a new file writes.
fn saved_alone(arena: &sue_core::Arena) -> Vec<u8> {
    let mut file = SueFile::new();
    crate::runtime::save::fill(
        &mut file,
        arena,
        &sue_core::pixels::PixelOptions::SAVE,
        None,
    );
    file.to_bytes()
}

#[test]
fn painting_goes_on_while_a_save_runs_and_the_file_equals_a_save_in_place() {
    let (mut runtime, root) = with_picture("save-job", false);
    paint(&mut runtime, 10, 200);
    let before = runtime.engine.world.arena.clone();
    let path = root.join("busy.sue");
    let (go, gate) = std::sync::mpsc::channel();
    runtime.save_gate = Some(gate);
    runtime.start_save(&path);
    // The job waits at the gate: the save is running, and painting goes
    // on in the meantime, untouched by it and not touching it.
    assert!(runtime.is_saving());
    let copies = sue_core::shared_copies();
    paint(&mut runtime, 40, 90);
    assert_eq!(
        sue_core::shared_copies() - copies,
        1,
        "the stroke copies the snapshot's layer once"
    );
    // A second save asked for meanwhile follows the first.
    let later = root.join("later.sue");
    runtime.start_save(&later);
    go.send(()).unwrap();
    runtime.wait_jobs();
    assert!(!runtime.is_saving());
    // The file holds the document as it was when Save was chosen, byte
    // for byte what a save on this thread writes.
    let written = std::fs::read(&path).unwrap();
    assert_eq!(written, saved_alone(&before));
    // The later save holds the stroke made meanwhile.
    let after = std::fs::read(&later).unwrap();
    assert_eq!(after, saved_alone(&runtime.engine.world.arena));
    assert_ne!(after, written);
    assert_eq!(runtime.project(), Some(later.as_path()));
    // Each save is in the transfers list, done.
    let names: Vec<(String, bool)> = runtime
        .transfers
        .list()
        .iter()
        .filter(|transfer| transfer.label.ends_with(".sue"))
        .map(|transfer| {
            (
                transfer.label.clone(),
                transfer.state == sue_wm::TransferState::Done,
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("later.sue".to_owned(), true),
            ("busy.sue".to_owned(), true)
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_save_that_fails_at_any_byte_leaves_the_old_file_whole() {
    let (mut runtime, root) = with_picture("save-fails", false);
    let path = root.join("safe.sue");
    runtime.save_project_to(&path).unwrap();
    let old = std::fs::read(&path).unwrap();
    paint(&mut runtime, 20, 70);
    let mut file = SueFile::new();
    crate::runtime::save::fill(
        &mut file,
        &runtime.engine.world.arena,
        &sue_core::pixels::PixelOptions::SAVE,
        None,
    );
    let new = file.to_bytes();
    assert_ne!(new, old);
    // A write that stops at any byte offset of the new file reports it
    // (in memory, every offset), so the rename never comes...
    for offset in 0..new.len() as u64 {
        let mut sink = Vec::new();
        let mut cut = Cut {
            out: &mut sink,
            left: offset,
        };
        assert!(
            file.write_to(&mut cut).is_err(),
            "a write cut at byte {offset} passed for whole"
        );
    }
    // ...and on disk, at offsets all through the file (the first and last
    // bytes, every chunk's start, and a stride between), the old file is
    // still all there, with nothing left beside it.
    let len = new.len() as u64;
    let mut offsets: Vec<u64> = (0..16).chain(len - 16..len).collect();
    offsets.extend((0..len).step_by(997));
    // The header is what an empty file holds before its end chunk.
    let mut at = SueFile::new().to_bytes().len() - 20;
    while at + 16 <= new.len() {
        offsets.extend([at as u64, at as u64 + 15]);
        let length = u64::from_le_bytes(new[at + 8..at + 16].try_into().unwrap()) as usize;
        at += 16 + length + 4;
    }
    for offset in offsets {
        assert!(crate::runtime::save::write_sue(&path, &file, Some(offset)).is_err());
        assert!(
            std::fs::read(&path).unwrap() == old,
            "a write that failed at byte {offset} changed the old file"
        );
    }
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    // Through the runtime: the failure is said, the project stays as it
    // was, and the next save (that works) writes the new file.
    runtime.fail_save_after = Some(100);
    runtime.start_save(&path);
    runtime.wait_jobs();
    assert_eq!(std::fs::read(&path).unwrap(), old);
    assert!(runtime.saved_at != Some(runtime.engine.world.arena.revision()));
    runtime.fail_save_after = None;
    runtime.save_project_to(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), new);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_save_writes_a_preview_and_a_file_without_one_gets_one() {
    let (mut runtime, root) = with_picture("preview", false);
    // A project saved without a preview, as older builds and other
    // programs may write it.
    let bare = root.join("bare.sue");
    let mut file = SueFile::new();
    file.set_arena(&runtime.engine.world.arena);
    assert!(file.chunk(THUMBNAIL).is_none());
    std::fs::write(&bare, file.to_bytes()).unwrap();
    runtime
        .open_project(&bare, &std::fs::read(&bare).unwrap())
        .unwrap();
    runtime.save_project_to(&bare).unwrap();
    let bytes = std::fs::read(&bare).unwrap();
    let thumb = sue_wm::files::thumbs::preview(&bytes, sue_wm::files::thumbs::SIDE)
        .expect("the saved project has a preview");
    // Square Post is square: a square card, of the paper's white.
    assert_eq!(thumb.size[0], thumb.size[1]);
    assert!(thumb.size[0] <= sue_wm::files::thumbs::SIDE);
    assert_eq!(&thumb.pixels[..4], &[255, 255, 255, 255]);
    // The preview follows the picture: a dark disc in the middle.
    let handle = first_raster(&runtime);
    let entity = runtime.engine.world.arena.get_mut(handle).unwrap();
    if let Some(ComponentData::RasterTile2D(raster)) =
        entity.component_mut(ComponentKind::RasterTile2D)
    {
        let (width, height) = (raster.width() as i64, raster.height() as i64);
        let radius = width.min(height) / 3;
        let bytes = raster.bytes_mut().unwrap();
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = (x - width / 2, y - height / 2);
                if dx * dx + dy * dy < radius * radius {
                    let at = ((y * width + x) * 4) as usize;
                    bytes[at..at + 4].copy_from_slice(&[30, 60, 160, 255]);
                }
            }
        }
    }
    runtime.save_project_to(&bare).unwrap();
    let again = std::fs::read(&bare).unwrap();
    shoot("project-card.png", &again);
    let painted = sue_wm::files::thumbs::preview(&again, 1024).unwrap();
    assert_ne!(painted.pixels, thumb.pixels);
    // Saving again with nothing changed writes the same bytes, and
    // re-encodes nothing.
    runtime.save_project_to(&bare).unwrap();
    assert_eq!(std::fs::read(&bare).unwrap(), again);
    assert_eq!(runtime.project_file.last_save().encoded, 0);
    let _ = std::fs::remove_dir_all(&root);
}

/// Writes the preview of the files browser for `bytes` as a PNG under the
/// plan's shots, to look at.
fn shoot(name: &str, bytes: &[u8]) {
    let Some(thumb) = sue_wm::files::thumbs::preview(bytes, sue_wm::files::thumbs::SIDE) else {
        return;
    };
    let dir = PathBuf::from(r"N:\Sue\plan\v0.0.8\shots\p3-9");
    if !dir.parent().is_some_and(Path::exists) {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(png) = sue_io::encode_png_rgba(thumb.size[0], thumb.size[1], &thumb.pixels) {
        let _ = std::fs::write(dir.join(name), png);
    }
}

#[test]
fn unsaved_work_is_kept_as_a_journal_and_recovers_from_a_cut() {
    let (mut runtime, root) = with_picture("journal", true);
    let start = Instant::now();
    // The first keep writes the work whole, with its preview.
    paint(&mut runtime, 0, 30);
    runtime.autosave();
    runtime.wait_jobs();
    let own = runtime.work.own_id(0);
    let kept = runtime.work.autosave(&own).cloned().unwrap();
    assert!(kept.file.exists() && !kept.journal().exists());
    shoot("kept-work-card.png", &std::fs::read(&kept.file).unwrap());
    let whole = std::fs::read(&kept.file).unwrap();
    // The next ones add records of what changed.
    paint(&mut runtime, 100, 60);
    runtime.autosave();
    runtime.wait_jobs();
    let after_two = runtime.engine.world.arena.clone();
    paint(&mut runtime, 200, 90);
    runtime.autosave();
    runtime.wait_jobs();
    assert_eq!(
        std::fs::read(&kept.file).unwrap(),
        whole,
        "the whole file stays"
    );
    let journal = std::fs::read(kept.journal()).unwrap();
    assert!(
        journal.len() < whole.len(),
        "two records ({} bytes) cost less than the work ({} bytes)",
        journal.len(),
        whole.len()
    );
    // Another run, after a crash cut the last record short, offers the
    // work as the records before it left it.
    std::fs::write(kept.journal(), &journal[..journal.len() - 5]).unwrap();
    let host_dir = runtime.engine.paths.data_dir.clone();
    drop(runtime);
    let (other, other_root) = super::runtime("journal-next");
    let mut other = other;
    other.engine.paths.data_dir = host_dir;
    let mut other = other.remembering(true);
    let offered = other.work.unsaved().next().cloned().expect("offered");
    assert_eq!(offered.id, kept.id);
    other.recover(&offered.id, &mut NoTextures, start);
    let id = after_two.get(first_raster_in(&after_two)).unwrap().id();
    let layer = |arena: &sue_core::Arena| {
        arena
            .get_by_id(id)
            .unwrap()
            .component(ComponentKind::RasterTile2D)
            .and_then(ComponentData::as_raster)
            .cloned()
            .unwrap()
    };
    assert_eq!(layer(&other.engine.world.arena), layer(&after_two));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&other_root);
}

#[test]
fn work_kept_by_sue_0_0_75_is_offered_and_recovers() {
    let (runtime, root) = super::runtime("golden-work");
    let mut runtime = runtime;
    // The work folder exactly as 0.0.75 left it (fixture (i) of P0-4).
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sue-paint/tests/golden/work/autosave");
    let data = root.join("kept-data");
    let autosave = data.join("work").join("autosave");
    std::fs::create_dir_all(&autosave).unwrap();
    for entry in std::fs::read_dir(&golden).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), autosave.join(entry.file_name())).unwrap();
    }
    runtime.engine.paths.data_dir = data;
    let mut runtime = runtime.remembering(true);
    let offered: Vec<(String, String)> = runtime
        .work
        .unsaved()
        .map(|kept| (kept.id.clone(), kept.title()))
        .collect();
    assert_eq!(
        offered,
        [
            ("1760000500-20276-1".to_owned(), "Illustration".to_owned()),
            (
                "1760000400-20276-1".to_owned(),
                "The Lighthouse Keeper".to_owned()
            )
        ]
    );
    runtime.recover("1760000500-20276-1", &mut NoTextures, Instant::now());
    let names: Vec<String> = runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.name().to_owned())
        .collect();
    for name in ["Illustration", "Paper", "Layer 1"] {
        assert!(names.contains(&name.to_owned()), "{name} in {names:?}");
    }
    assert_eq!(
        runtime.shell.page().map(AppletId::as_str),
        Some(sue_paint::ID)
    );
    // Kept again by this build, it stays readable as kept work.
    paint(&mut runtime, 1, 5);
    runtime.autosave();
    runtime.wait_jobs();
    let kept = runtime
        .work
        .autosave("1760000500-20276-1")
        .cloned()
        .unwrap();
    assert!(kept.load().is_ok());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_empty_a4_of_twenty_layers_is_small_and_a_keep_costs_the_interface_little() {
    let (runtime, root) = super::runtime("bounds");
    let mut runtime = runtime.remembering(true);
    // A4 at 350 dpi, twenty empty layers (new layers share nothing, but
    // empty they cost the same; one buffer keeps the test's memory low).
    let empty = sue_core::RasterTile2D::new(2894, 4093, sue_core::PixelFormat::Rgba8).unwrap();
    for index in 0..20 {
        runtime
            .engine
            .world
            .arena
            .spawn(format!("Layer {index}"), empty.clone());
    }
    drop(empty);
    let path = root.join("a4.sue");
    std::fs::create_dir_all(&root).unwrap();
    let started = std::time::Instant::now();
    runtime.save_project_to(&path).unwrap();
    let full = started.elapsed();
    let size = std::fs::metadata(&path).unwrap().len();
    println!("empty A4, 20 layers: {size} bytes, saved in {full:?}");
    assert!(size < 1 << 20, "{size} bytes");
    // One stroke, then a keep: the interface thread only takes the
    // snapshot and starts the job.
    let handle = runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(handle, _)| handle)
        .next()
        .unwrap();
    let entity = runtime.engine.world.arena.get_mut(handle).unwrap();
    if let Some(ComponentData::RasterTile2D(raster)) =
        entity.component_mut(ComponentKind::RasterTile2D)
    {
        raster.bytes_mut_in([1000, 1000, 40, 40]).unwrap()[1000 * 2894 * 4 + 4000] = 9;
    }
    let started = std::time::Instant::now();
    runtime.autosave();
    let on_the_interface = started.elapsed();
    assert!(runtime.keeping.is_some(), "the keep runs on a job");
    runtime.wait_jobs();
    println!("a keep after one stroke: {on_the_interface:?} on the interface thread");
    assert!(
        on_the_interface < Duration::from_millis(16),
        "{on_the_interface:?} is more than a frame"
    );
    let _ = std::fs::remove_dir_all(&root);
}
