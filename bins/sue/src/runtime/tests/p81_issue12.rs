//! Sue 0.0.81, the suggestions of issue 12 for Sue Paint: several layers
//! picked and moved together, several pictures brought in at once, a
//! layer in an animation folder shown on the timeline at once, an
//! existing layer placed on a frame, the timeline's thumbnail sizes and
//! the shortcuts. Each test drives real frames of the running Sue Paint.

use std::sync::Arc;

use super::paint_anim::{arena_bytes, line_on_canvas, open_picture};
use super::*;

fn arena(paint: &PaintDriver) -> &sue_core::Arena {
    &paint.runtime.engine.world.arena
}

/// The layers of the open picture, bottom first.
fn layers(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(arena(paint), open_picture(paint))
}

fn names(paint: &PaintDriver) -> Vec<String> {
    layers(paint).into_iter().map(|info| info.name).collect()
}

fn depth(paint: &PaintDriver) -> usize {
    paint.runtime.engine.history.depth()
}

/// Runs `work` as one undo step: undo brings back the arena as it was
/// and redo the arena as `work` left it.
fn one_step(paint: &mut PaintDriver, what: &str, work: impl FnOnce(&mut PaintDriver)) {
    let (before, steps) = (arena_bytes(paint), depth(paint));
    work(paint);
    paint.settle(4);
    let after = arena_bytes(paint);
    assert!(after != before, "{what} changes the document");
    assert_eq!(depth(paint), steps + 1, "{what} is one undo step");
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(paint) == before, "{what} undone");
    paint.command("edit.redo");
    paint.settle(2);
    assert!(arena_bytes(paint) == after, "{what} done again");
}

/// One frame with `events` while `modifiers` are held.
fn frame_with_keys(paint: &mut PaintDriver, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
    let mut events = events;
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    paint.step(events);
}

/// A click at `at` with `modifiers` held.
fn click_with(paint: &mut PaintDriver, at: Pos2, modifiers: egui::Modifiers) {
    let event = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    frame_with_keys(paint, vec![egui::Event::PointerMoved(at)], modifiers);
    frame_with_keys(paint, vec![event(true)], modifiers);
    frame_with_keys(paint, vec![event(false)], modifiers);
    frame_with_keys(paint, Vec::new(), egui::Modifiers::NONE);
    paint.settle(4);
}

/// The Layer palette's row of the layer named `name`.
fn row(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    paint.settle(2);
    paint
        .rect_of(name)
        .unwrap_or_else(|| panic!("{name:?} is in the list: {:?}", paint.texts))
}

/// A small picture file in the test's folder.
fn picture_file(paint: &PaintDriver, name: &str) -> PathBuf {
    let pixels: Vec<u8> = (0..16).flat_map(|_| [200, 40, 40, 255]).collect();
    let png = sue_io::encode_png_rgba(4, 4, &pixels).unwrap();
    std::fs::create_dir_all(&paint.root).unwrap();
    let path = paint.root.join(name);
    std::fs::write(&path, png).unwrap();
    path
}

/// Answers the file dialog Sue Paint asked for with `files`, as a
/// dialog that lets several be chosen does.
fn choose(paint: &mut PaintDriver, files: &[PathBuf]) {
    let mut pick = paint
        .runtime
        .pending_pick()
        .cloned()
        .expect("a file dialog was asked for");
    assert!(pick.many, "the dialog lets several files be chosen");
    pick.also = files[1..].to_vec();
    paint.runtime.request(
        ShellRequest::Picked {
            pick,
            path: files[0].clone(),
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(6);
}

fn shows(paint: &PaintDriver, text: &str) -> bool {
    paint.texts.iter().any(|(shown, _)| shown == text)
}

/// The picture's timeline, cloned.
fn timeline(paint: &PaintDriver) -> sue_model::Sequence {
    sue_paint::anim::timeline(arena(paint), open_picture(paint))
        .expect("a timeline")
        .clone()
}

/// The cel the first animation folder shows at `frame` (0 based).
fn shown_at(paint: &PaintDriver, frame: u32) -> Option<sue_core::EntityId> {
    let sequence = timeline(paint);
    let track = sue_paint::anim::folders(&sequence)[0];
    sue_paint::anim::cel_at(&sequence.tracks[track], sequence.rate, frame)
}

fn named(paint: &PaintDriver, name: &str) -> sue_core::EntityId {
    layers(paint)
        .into_iter()
        .find(|info| info.name == name)
        .map(|info| info.id)
        .unwrap_or_else(|| panic!("a layer named {name:?}: {:?}", names(paint)))
}

// ----- (1) Several layers picked and moved together -------------------------

/// Issue 12, 1: Shift+click and Ctrl+click pick several layers; the Move
/// Layer tool moves them all on the canvas, each keeping its place
/// beside the others, as one undo step; Duplicate copies them all.
#[test]
fn five_picked_layers_move_together_and_undo_as_one() {
    let mut paint = PaintDriver::new("p81-issue12-move");
    line_on_canvas(&mut paint, -80.0);
    for index in 1..5 {
        paint.command("layer.new");
        paint.settle(2);
        line_on_canvas(&mut paint, -80.0 + 40.0 * index as f32);
    }
    let five: Vec<sue_paint::doc::LayerInfo> = layers(&paint)
        .into_iter()
        .filter(|info| info.kind == sue_model::picture::LayerKind::Raster && info.name != "Paper")
        .collect();
    let five = five[five.len() - 5..].to_vec();
    // The top one is current: Shift+click on the lowest picks the range,
    // and Ctrl+click takes one out and puts it back.
    let lowest = row(&mut paint, &five[0].name);
    click_with(&mut paint, lowest.center(), egui::Modifiers::SHIFT);
    let middle_row = row(&mut paint, &five[2].name);
    click_with(&mut paint, middle_row.center(), egui::Modifiers::COMMAND);
    click_with(&mut paint, middle_row.center(), egui::Modifiers::COMMAND);
    let offsets = |paint: &PaintDriver| -> Vec<[i64; 2]> {
        let now = layers(paint);
        five.iter()
            .map(|info| {
                now.iter()
                    .find(|other| other.id == info.id)
                    .expect("the layer")
                    .style
                    .offset
            })
            .collect()
    };
    let before = offsets(&paint);
    paint.command("tool.move_layer");
    paint.settle(2);
    let middle = paint_middle(&paint.runtime);
    one_step(&mut paint, "moving five layers", |paint| {
        paint.drag(
            middle + vec2(-40.0, -80.0),
            middle + vec2(20.0, -40.0),
            egui::PointerButton::Primary,
        );
    });
    let after = offsets(&paint);
    let moved: Vec<[i64; 2]> = before
        .iter()
        .zip(&after)
        .map(|(was, is)| [is[0] - was[0], is[1] - was[1]])
        .collect();
    assert_ne!(moved[0], [0, 0], "the layers moved");
    assert!(
        moved.iter().all(|by| *by == moved[0]),
        "every picked layer moved by the same: {moved:?}"
    );
    // Duplicate copies all five, as one step.
    let count = layers(&paint).len();
    one_step(&mut paint, "duplicating five layers", |paint| {
        paint.command("layer.duplicate");
    });
    assert_eq!(layers(&paint).len(), count + 5);
}

// ----- (2) Several pictures at once -----------------------------------------

/// Issue 12, 2: several files chosen in the dialog ask what they become;
/// as layers they come in by natural file name order, in one undo step.
#[test]
fn several_pictures_chosen_at_once_become_layers_in_one_step() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-issue12-layers");
    let files: Vec<PathBuf> = ["img10.png", "img2.png", "img1.png"]
        .iter()
        .map(|name| picture_file(&paint, name))
        .collect();
    let before = names(&paint);
    paint.command("file.import.image");
    paint.settle(2);
    one_step(&mut paint, "importing three pictures", |paint| {
        choose(paint, &files);
        assert!(shows(paint, "Import Pictures"), "{:?}", paint.texts);
        assert!(shows(paint, "Separate layers of this canvas"));
        assert!(shows(paint, "Frames of a new animation folder"));
        assert!(shows(paint, "Separate canvases"));
        assert_eq!(names(paint), before, "nothing comes in before OK");
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    });
    let now = names(&paint);
    assert_eq!(
        now[before.len()..],
        ["img1", "img2", "img10"],
        "in the order people count: {now:?}"
    );
}

/// Issue 12, 2: the same pictures as frames of an animation folder, one
/// frame after another, in one undo step.
#[test]
fn several_pictures_become_the_frames_of_an_animation_folder() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-issue12-frames");
    let files: Vec<PathBuf> = ["walk_10.png", "walk_2.png", "walk_1.png"]
        .iter()
        .map(|name| picture_file(&paint, name))
        .collect();
    paint.command("file.import.image");
    paint.settle(2);
    one_step(&mut paint, "importing three frames", |paint| {
        choose(paint, &files);
        paint.click_text("Frames of a new animation folder");
        paint.settle(2);
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    });
    let shown: Vec<String> = (0..3)
        .map(|frame| {
            let cel = shown_at(&paint, frame).expect("a cel on the frame");
            arena(&paint).get_by_id(cel).unwrap().name().to_owned()
        })
        .collect();
    assert_eq!(shown, ["walk_1", "walk_2", "walk_10"]);
    assert!(
        names(&paint).iter().any(|name| name == "walk"),
        "{:?}",
        names(&paint)
    );
}

/// A file dropped on Sue's window, as the system hands it over.
#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

/// One frame of Sue's window with `events` and the files `dropped`.
fn drop_frame(paint: &mut PaintDriver, events: Vec<egui::Event>, dropped: &[PathBuf]) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        dropped_files: dropped
            .iter()
            .map(|path| Arc::new(Dropped(path.clone())) as egui::DroppedFileHandle)
            .collect(),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let (runtime, now) = (&mut paint.runtime, paint.now);
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    paint.texts = text_rects(&output);
    output.drop_without_applying_deltas();
}

/// Issue 12, 2: twenty files dropped on the canvas together ask once
/// and become twenty layers in one undo step.
#[test]
fn twenty_pictures_dropped_together_become_twenty_layers_in_one_step() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-issue12-drop");
    let files: Vec<PathBuf> = (1..=20)
        .rev()
        .map(|index| picture_file(&paint, &format!("page{index}.png")))
        .collect();
    let before = names(&paint);
    let canvas = paint_middle(&paint.runtime);
    one_step(&mut paint, "dropping twenty pictures", |paint| {
        drop_frame(paint, vec![egui::Event::PointerMoved(canvas)], &[]);
        drop_frame(paint, Vec::new(), &files);
        paint.settle(6);
        assert!(shows(paint, "Import Pictures"), "{:?}", paint.texts);
        assert_eq!(names(paint), before, "the dialog asks first");
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    });
    let now = names(&paint);
    let wanted: Vec<String> = (1..=20).map(|index| format!("page{index}")).collect();
    assert_eq!(now[before.len()..], wanted[..], "{now:?}");
}

// ----- (3) A layer in an animation folder shows on the timeline -------------

/// Issue 12, 3: a picture that comes in as a layer of an animation
/// folder is a cel on its timeline at once, at the frame shown, in the
/// same undo step.
#[test]
fn a_layer_put_in_an_animation_folder_is_a_cel_at_the_frame_shown() {
    let mut paint = PaintDriver::new("p81-issue12-cel");
    paint.new_timeline();
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    paint.settle(2);
    let first = shown_at(&paint, 0).expect("the first cel");
    for _ in 0..3 {
        paint.command("anim.next");
    }
    paint.settle(2);
    assert_eq!(shown_at(&paint, 3), Some(first), "held until now");
    let file = picture_file(&paint, "pose.png");
    paint.command("file.import.image");
    paint.settle(2);
    one_step(&mut paint, "importing into the folder", |paint| {
        choose(paint, std::slice::from_ref(&file));
    });
    let pose = named(&paint, "pose");
    assert_eq!(
        sue_paint::anim::cel_folder(arena(&paint), pose),
        sue_paint::anim::cel_folder(arena(&paint), first),
        "the picture is a cel of the folder"
    );
    assert_eq!(shown_at(&paint, 3), Some(pose), "shown from the frame");
    assert_eq!(shown_at(&paint, 0), Some(first), "the frames before stay");
}

// ----- (4) An existing layer placed on a frame ------------------------------

/// A timeline with an animation folder holding one cel, and a layer
/// named `Extra` outside the folder; the frame shown is the fourth.
fn folder_and_a_layer_outside(paint: &mut PaintDriver) -> sue_core::EntityId {
    paint.command("layer.new");
    paint.settle(2);
    let extra = layers(paint).last().expect("the new layer").id;
    paint.new_timeline();
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    paint.settle(2);
    for _ in 0..3 {
        paint.command("anim.next");
    }
    paint.settle(2);
    assert_eq!(sue_paint::anim::cel_folder(arena(paint), extra), None);
    extra
}

/// Issue 12, 4: Place Layer Here (the cel's right click list) puts a
/// layer from outside the folder on the frame, as one undo step.
#[test]
fn place_layer_here_puts_an_existing_layer_on_the_frame() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-issue12-place");
    let extra = folder_and_a_layer_outside(&mut paint);
    let first = shown_at(&paint, 0).expect("the first cel");
    one_step(&mut paint, "placing the layer", |paint| {
        paint.command(&format!("anim.cel.place.{extra}"));
    });
    assert_eq!(
        sue_paint::anim::cel_folder(arena(&paint), extra),
        sue_paint::anim::cel_folder(arena(&paint), first),
        "it went into the folder"
    );
    assert_eq!(shown_at(&paint, 3), Some(extra));
    assert_eq!(shown_at(&paint, 2), Some(first));
}

/// Issue 12, 4: a layer dragged from the Layer palette onto a frame of
/// the folder's row of the timeline is placed there.
#[test]
fn a_layer_dragged_onto_a_timeline_frame_is_placed_there() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p81-issue12-drag");
    let extra = folder_and_a_layer_outside(&mut paint);
    let first = shown_at(&paint, 0).expect("the first cel");
    let name = arena(&paint).get_by_id(extra).unwrap().name().to_owned();
    let folder = sue_paint::anim::cel_folder(arena(&paint), first).expect("the folder");
    let folder_name = arena(&paint).get_by_id(folder).unwrap().name().to_owned();
    let texts = paint.settle(4);
    let header = texts
        .iter()
        .find(|(text, rect)| text == "Timeline" && rect.top() > 500.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the Timeline palette: {texts:?}"));
    // The folder's row on the timeline, and the column of frame 6: the
    // ruler numbers frames 1 and 7.
    let track = texts
        .iter()
        .find(|(text, rect)| on_timeline(text, &folder_name, rect, &header))
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the folder's track: {texts:?}"));
    let seven = texts
        .iter()
        .find(|(text, rect)| text == "7" && rect.top() > header.bottom() && rect.left() > 560.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("frame 7 on the ruler: {texts:?}"));
    let one = texts
        .iter()
        .find(|(text, rect)| {
            text == "1" && (rect.top() - seven.top()).abs() < 1.0 && rect.left() > 560.0
        })
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("frame 1 on the ruler: {texts:?}"));
    let column = (seven.left() - one.left()) / 6.0;
    let from = texts
        .iter()
        .find(|(text, rect)| *text == name && rect.left() > 1100.0)
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| panic!("the layer's row: {texts:?}"));
    // The middle of frame 6's column.
    let to = egui::pos2(one.left() + column * 5.5, track.center().y);
    one_step(&mut paint, "dragging the layer onto the frame", |paint| {
        paint.drag(from, to, egui::PointerButton::Primary);
    });
    assert_eq!(
        sue_paint::anim::cel_folder(arena(&paint), extra),
        Some(folder),
        "it went into the folder"
    );
    let at: Vec<u32> = (0..12)
        .filter(|frame| shown_at(&paint, *frame) == Some(extra))
        .collect();
    assert!(at.contains(&5), "shown from frame 6 on: {at:?}");
    assert!(!at.contains(&3), "not before it: {at:?}");
}

/// `true` for the text of the timeline's row of the track named `name`
/// (it reads the name and the cel shown), under the palette's `header`.
fn on_timeline(text: &str, name: &str, rect: &egui::Rect, header: &egui::Rect) -> bool {
    text.starts_with(name) && rect.top() > header.bottom() && rect.left() < 560.0
}

// ----- (6) and (7): what Sue Paint already had ------------------------------

/// How far apart the timeline draws the rows of the two animation
/// folders.
fn rows_apart(paint: &mut PaintDriver) -> f32 {
    let folders: Vec<String> = layers(paint)
        .into_iter()
        .filter(|info| sue_paint::anim::is_animation_folder(arena(paint), info.id))
        .map(|info| info.name)
        .collect();
    let texts = paint.settle(4);
    let header = texts
        .iter()
        .find(|(text, rect)| text == "Timeline" && rect.top() > 500.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the Timeline palette: {texts:?}"));
    let rows: Vec<f32> = folders
        .iter()
        .map(|name| {
            texts
                .iter()
                .find(|(text, rect)| on_timeline(text, name, rect, &header))
                .map(|(_, rect)| rect.center().y)
                .unwrap_or_else(|| panic!("the track of {name}: {texts:?}"))
        })
        .collect();
    (rows[0] - rows[1]).abs()
}

/// Issue 12, 6: the timeline's menu sets the size of the cels' pictures,
/// from none to the largest, and the rows grow with it.
#[test]
fn the_timeline_menu_sets_the_thumbnail_size() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let registry = sue_paint::registry();
    for size in ["none", "smallest", "small", "medium", "large", "largest"] {
        assert!(
            registry.has(&format!("anim.thumbnails.{size}")),
            "Thumbnail Size, {size}"
        );
    }
    let mut paint = PaintDriver::new("p81-issue12-thumbs");
    paint.new_timeline();
    for _ in 0..2 {
        paint.command("anim.layer.folder");
        paint.command("anim.cel.new");
        paint.settle(2);
    }
    paint.command("anim.thumbnails.none");
    let flat = rows_apart(&mut paint);
    paint.command("anim.thumbnails.largest");
    let tall = rows_apart(&mut paint);
    assert!(tall > flat + 8.0, "the rows grow: {flat} to {tall}");
}

/// Issue 12, 7: Sue Paint comes with shortcuts, and File, Shortcut
/// Settings opens Sue's Settings on them, where each can be changed.
#[test]
fn shortcuts_come_set_and_can_be_changed() {
    let keys = sue_paint::registry().shortcuts();
    assert!(keys.len() > 100, "a full set of keys: {}", keys.len());
    for command in ["edit.undo", "file.save", "layer.new", "view.zoom_in"] {
        assert!(
            keys.iter().any(|shortcut| shortcut.command == command),
            "{command} has a key"
        );
    }
    let mut paint = PaintDriver::new("p81-issue12-keys");
    assert!(paint.runtime.shell.window_rect().is_none());
    paint.command("file.shortcuts");
    paint.settle(30);
    assert!(
        paint.runtime.shell.window_rect().is_some(),
        "Shortcut Settings opened"
    );
}
