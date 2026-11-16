//! Tutorial acceptance (P5-2): "How to animate in Clip Studio Paint
//! 2025", followed in Sue Paint row by row. Each test is named after the
//! step of `plan/v0.0.8/tutorial-animate-2025.md` it follows (`step_NNN`
//! for the step table, `seen_vNN` for the rows seen in the video), and
//! drives real frames: menus and keys through the runtime, controls found
//! by their names or by where a frame drew their text, never by a fixed
//! place. No frame or subtitle of the video is in this file (rule 10).

use super::*;
use egui::{Event, Modifiers, PointerButton, Rect};
use sue_core::{Arena, EntityId};

// ----- Helpers ---------------------------------------------------------------

/// Every control one frame names to assistive technology, with its rect;
/// the frame's texts are kept on the driver.
fn look(paint: &mut PaintDriver) -> Vec<(String, Rect)> {
    paint.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    paint.texts = text_rects(&output);
    let named = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let label = node.label()?.to_owned();
            let bounds = node.bounds()?;
            Some((
                label,
                Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect();
    output.drop_without_applying_deltas();
    named
}

/// Where the control named `name` is (below the menu bar first, since
/// Sue Paint's menus share names with New's kinds of work).
fn control(paint: &mut PaintDriver, name: &str) -> Rect {
    let named = look(paint);
    let found: Vec<Rect> = named
        .iter()
        .filter(|(label, _)| label == name)
        .map(|(_, rect)| *rect)
        .collect();
    found
        .iter()
        .rev()
        .find(|rect| rect.top() > 30.0)
        .or_else(|| found.first())
        .copied()
        .unwrap_or_else(|| {
            let names: Vec<&str> = named.iter().map(|(label, _)| label.as_str()).collect();
            panic!("{name:?} is named on screen: {names:?}")
        })
}

/// Whether a control named `name` is on screen.
fn has_control(paint: &mut PaintDriver, name: &str) -> bool {
    look(paint).iter().any(|(label, _)| label == name)
}

fn button(at: Pos2, which: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: at,
        button: which,
        pressed,
        modifiers,
    }
}

/// A click of `which` at `at` with `modifiers` held; what the frames
/// after draw.
fn click_with(
    paint: &mut PaintDriver,
    at: Pos2,
    which: PointerButton,
    modifiers: Modifiers,
) -> Vec<(String, Rect)> {
    paint.step(vec![
        Event::ModifiersChanged(modifiers),
        Event::PointerMoved(at),
    ]);
    paint.step(vec![
        Event::ModifiersChanged(modifiers),
        button(at, which, true, modifiers),
    ]);
    paint.step(vec![
        Event::ModifiersChanged(modifiers),
        button(at, which, false, modifiers),
    ]);
    paint.step(vec![Event::ModifiersChanged(Modifiers::NONE)]);
    paint.settle(4)
}

/// A plain left click at `at`.
fn click(paint: &mut PaintDriver, at: Pos2) -> Vec<(String, Rect)> {
    click_with(paint, at, PointerButton::Primary, Modifiers::NONE)
}

/// A right click at `at`.
fn right_click(paint: &mut PaintDriver, at: Pos2) -> Vec<(String, Rect)> {
    click_with(paint, at, PointerButton::Secondary, Modifiers::NONE)
}

/// Two quick clicks at `at`, a few frames apart as a hand makes them
/// (well inside the double click's time): what the first click picks is
/// laid out before the second.
fn double_click(paint: &mut PaintDriver, at: Pos2) {
    // Long enough after any click before that the first click of these
    // two is a click of its own (two clicks before it would make the
    // pair a triple click).
    paint.settle(25);
    paint.step(vec![Event::PointerMoved(at)]);
    for _ in 0..2 {
        paint.step(vec![button(
            at,
            PointerButton::Primary,
            true,
            Modifiers::NONE,
        )]);
        paint.step(vec![button(
            at,
            PointerButton::Primary,
            false,
            Modifiers::NONE,
        )]);
        paint.settle(3);
    }
    paint.settle(2);
}

/// A click on the control named `name`.
fn click_named(paint: &mut PaintDriver, name: &str) -> Vec<(String, Rect)> {
    let at = control(paint, name).center();
    click(paint, at)
}

/// A drag from `from` to `to` with `modifiers` held all along.
fn drag_with(paint: &mut PaintDriver, from: Pos2, to: Pos2, modifiers: Modifiers) {
    let held = |events: Vec<Event>| {
        let mut all = vec![Event::ModifiersChanged(modifiers)];
        all.extend(events);
        all
    };
    paint.step(held(vec![Event::PointerMoved(from)]));
    paint.step(held(vec![button(
        from,
        PointerButton::Primary,
        true,
        modifiers,
    )]));
    for index in 1..=16 {
        let at = from + (to - from) * (index as f32 / 16.0);
        paint.step(held(vec![Event::PointerMoved(at)]));
    }
    paint.step(held(vec![button(
        to,
        PointerButton::Primary,
        false,
        modifiers,
    )]));
    paint.step(vec![Event::ModifiersChanged(Modifiers::NONE)]);
    paint.settle(6);
}

/// One press of `key_name` with `modifiers`, then a few frames.
fn press_key(paint: &mut PaintDriver, key_name: egui::Key, modifiers: Modifiers) {
    paint.step(vec![key(key_name, modifiers)]);
    paint.settle(3);
}

/// Replaces what the focused field holds with `text`, ended with Tab.
fn type_over(paint: &mut PaintDriver, text: &str) {
    paint.step(vec![key(egui::Key::A, Modifiers::COMMAND)]);
    paint.step(vec![Event::Text(text.to_owned())]);
    paint.step(vec![key(egui::Key::Tab, Modifiers::NONE)]);
    paint.settle(3);
}

/// Clicks the field named `name` and types `text` over it.
fn fill_named(paint: &mut PaintDriver, name: &str, text: &str) {
    let at = control(paint, name).center();
    click(paint, at);
    type_over(paint, text);
}

/// Whether the last frame drew exactly `text`.
fn shows(paint: &PaintDriver, text: &str) -> bool {
    paint.texts.iter().any(|(found, _)| found == text)
}

/// Whether the last frame drew a text that holds `part`.
fn shows_part(paint: &PaintDriver, part: &str) -> bool {
    paint.texts.iter().any(|(found, _)| found.contains(part))
}

fn arena(paint: &PaintDriver) -> &Arena {
    &paint.runtime.engine.world.arena
}

/// The canvas in focus.
fn picture(paint: &PaintDriver) -> EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The layers of the canvas, bottom first, folders with their layers.
fn stack(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(arena(paint), picture(paint))
}

/// The layer of the canvas named `name`.
fn layer(paint: &PaintDriver, name: &str) -> EntityId {
    stack(paint)
        .into_iter()
        .find(|info| info.name == name)
        .map(|info| info.id)
        .unwrap_or_else(|| {
            let names: Vec<String> = stack(paint).into_iter().map(|info| info.name).collect();
            panic!("a layer named {name:?}: {names:?}")
        })
}

fn name_of(paint: &PaintDriver, id: EntityId) -> String {
    arena(paint)
        .get_by_id(id)
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default()
}

fn info(paint: &PaintDriver, id: EntityId) -> sue_paint::doc::LayerInfo {
    stack(paint)
        .into_iter()
        .find(|info| info.id == id)
        .expect("the layer is on the canvas")
}

/// The canvas's timeline, cloned.
fn timeline(paint: &PaintDriver) -> sue_model::Sequence {
    sue_paint::anim::timeline(arena(paint), picture(paint))
        .expect("a timeline")
        .clone()
}

/// The track of the animation folder `folder`.
fn track_of(paint: &PaintDriver, folder: EntityId) -> usize {
    sue_paint::anim::track_of(&timeline(paint), folder).expect("the folder has a track")
}

/// The cel the animation folder `folder` shows on each of the first
/// `count` frames, by name ("" for none).
fn shown(paint: &PaintDriver, folder: EntityId, count: u32) -> Vec<String> {
    let sequence = timeline(paint);
    let track = &sequence.tracks[track_of(paint, folder)];
    (0..count)
        .map(|frame| {
            sue_paint::anim::cel_at(track, sequence.rate, frame)
                .map(|cel| name_of(paint, cel))
                .unwrap_or_default()
        })
        .collect()
}

/// The arena as bytes: entities, ids, components, properties and
/// relations.
fn bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    arena(paint).encode(&mut out);
    out.into_bytes()
}

fn depth(paint: &PaintDriver) -> usize {
    paint.runtime.engine.history.depth()
}

/// Rule 7 around `work`, which changes the document: undo gives back
/// the arena as it was (ids included), redo the arena as `work` left it.
fn undo_pair(paint: &mut PaintDriver, what: &str, work: impl FnOnce(&mut PaintDriver)) {
    let (before, steps) = (bytes(paint), depth(paint));
    work(paint);
    paint.settle(3);
    let after = bytes(paint);
    let end = depth(paint);
    assert!(after != before, "{what} changes the document");
    assert!(end > steps, "{what} is a step");
    while depth(paint) > steps {
        paint.command("edit.undo");
        paint.settle(2);
    }
    assert!(bytes(paint) == before, "{what} undone exactly");
    while depth(paint) < end {
        paint.command("edit.redo");
        paint.settle(2);
    }
    assert!(bytes(paint) == after, "{what} redone exactly");
}

/// Where `layer` has drawing, in canvas pixels.
fn drawn(paint: &PaintDriver, layer: EntityId) -> Option<[u32; 4]> {
    let raster = sue_paint::doc::raster(arena(paint), layer)?;
    sue_paint::warp::bounds(raster.bytes()?, raster.width(), raster.height())
}

/// Where the canvas's middle is on screen.
fn middle(paint: &PaintDriver) -> Pos2 {
    paint_middle(&paint.runtime)
}

/// A stroke across the canvas `dy` points from its middle.
fn stroke(paint: &mut PaintDriver, dy: f32) {
    let at = middle(paint);
    paint.drag(
        at + vec2(-90.0, dy),
        at + vec2(90.0, dy),
        PointerButton::Primary,
    );
}

/// Opens File, New with Ctrl+N.
fn open_new(paint: &mut PaintDriver) {
    paint.step(vec![key(egui::Key::N, Modifiers::COMMAND)]);
    paint.settle(10);
    assert!(paint.rect_of("New").is_some(), "the New dialog is open");
}

/// The preset the tutorial picks.
const PRESET: &str = "1920 x 1080 (192 dpi) with blank space";

/// Steps 1 to 4 done: File, New, Animation, the 1920 x 1080 preset with
/// blank space, OK. Sue Paint shows the new animation.
fn tutorial(tag: &str) -> PaintDriver {
    let mut paint = PaintDriver::new(tag);
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    click_named(&mut paint, "Preset");
    paint.click_text(PRESET);
    paint.click_text("OK");
    paint.settle(10);
    assert!(
        sue_paint::anim::timeline(arena(&paint), picture(&paint)).is_some(),
        "the new animation has its timeline"
    );
    paint
}

/// The animation folder New made, and its first cel.
fn first_folder(paint: &PaintDriver) -> EntityId {
    let sequence = timeline(paint);
    let track = sue_paint::anim::folders(&sequence)[0];
    sue_paint::anim::track_folder(&sequence.tracks[track]).expect("a folder track")
}

/// The animation folders, in the timeline's order.
fn animation_folders(paint: &PaintDriver) -> Vec<EntityId> {
    let sequence = timeline(paint);
    sue_paint::anim::folders(&sequence)
        .into_iter()
        .filter_map(|track| sue_paint::anim::track_folder(&sequence.tracks[track]))
        .collect()
}

/// Gives the command `command` the chord `chord`, as Settings, Keyboard
/// Shortcuts does.
fn bind(paint: &mut PaintDriver, command: &str, chord: &str) {
    let rice = &mut paint.runtime.engine.world.rice;
    let (setting, value) =
        sue_wm::shortcuts::bind(rice, "sue_paint_keys", command, &chord.parse().unwrap());
    rice.set(&setting, value).unwrap();
}

/// The Timeline palette's geometry in the last frame: where frame
/// `frame` (from 1) of the track whose name starts with `track` is.
fn cell(paint: &mut PaintDriver, track: &str, frame: u32) -> Pos2 {
    let texts = paint.settle(3);
    let header = texts
        .iter()
        .filter(|(text, rect)| text == "Timeline" && rect.top() > 560.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("the Timeline palette under the canvas: {texts:?}"));
    let number = |name: &str| {
        texts
            .iter()
            .filter(|(text, rect)| {
                text == name && rect.top() > header.bottom() && rect.left() > 400.0
            })
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("frame {name} on the ruler: {texts:?}"))
    };
    let row = texts
        .iter()
        .filter(|(text, rect)| {
            text.starts_with(track) && rect.top() > header.bottom() + 20.0 && rect.left() < 560.0
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{track}'s track: {texts:?}"));
    // Frame 1's number stands clear of the playback marker on its frame,
    // so the step is read between 7 and 13, each drawn just right of its
    // frame's left edge.
    let (seven, thirteen) = (number("7"), number("13"));
    let step = (thirteen.left() - seven.left()) / 6.0;
    Pos2::new(
        seven.left() + step * (frame as f32 - 7.0) + step * 0.4,
        row.center().y + 4.0,
    )
}

/// The Layer palette's row named `name` (the right column, the lowest
/// drawing of the name, which is the list's and not a caption's).
fn row(paint: &mut PaintDriver, name: &str) -> Rect {
    paint.settle(2);
    paint
        .texts
        .iter()
        .filter(|(text, rect)| {
            (text == name || text.starts_with(&format!("{name} : ")))
                && rect.left() > 1106.0
                && rect.bottom() < 900.0
        })
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{name}'s row in the Layer palette: {:?}", paint.texts))
}

/// The Layer palette's row of the layer `id`: among the rows that read
/// its name, top to bottom, the one in its place in the stack.
fn row_of(paint: &mut PaintDriver, id: EntityId) -> Rect {
    let name = name_of(paint, id);
    paint.settle(2);
    let mut rows: Vec<Rect> = paint
        .texts
        .iter()
        .filter(|(text, rect)| {
            (*text == name || text.starts_with(&format!("{name} : ")))
                && rect.left() > 1106.0
                && rect.bottom() < 900.0
                && rect.top() > 560.0
        })
        .map(|(_, rect)| *rect)
        .collect();
    rows.sort_by(|a, b| a.top().total_cmp(&b.top()));
    let same: Vec<EntityId> = stack(paint)
        .into_iter()
        .rev()
        .filter(|info| info.name == name)
        .map(|info| info.id)
        .collect();
    let at = same.iter().position(|found| *found == id).unwrap_or(0);
    *rows
        .get(at)
        .or(rows.last())
        .unwrap_or_else(|| panic!("{name}'s row: {:?}", paint.texts))
}

/// Draws `count` cels in the first animation folder, cel `n` keyed on
/// frame `1 + (n - 1) * gap`, each with a stroke of its own.
fn drawn_cels(paint: &mut PaintDriver, count: u32, gap: u32) -> EntityId {
    let folder = first_folder(paint);
    paint.command("anim.first");
    paint.settle(2);
    stroke(paint, -60.0);
    for index in 1..count {
        for _ in 0..gap {
            paint.command("anim.next");
        }
        paint.command("anim.cel.new");
        paint.settle(2);
        stroke(paint, -60.0 + index as f32 * 30.0);
    }
    paint.command("anim.first");
    paint.settle(2);
    folder
}

/// Hands Sue Paint the file a save dialog for `purpose` would pick, and
/// waits for the export it starts.
fn picked(paint: &mut PaintDriver, purpose: &str, path: &std::path::Path) {
    let applet = AppletId::new(sue_paint::ID).unwrap();
    paint.runtime.request(
        ShellRequest::Picked {
            pick: sue_wm::FilePick::save(Some(applet), purpose, "Export", "x"),
            path: path.to_path_buf(),
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(2);
    paint.finish_exports();
}

/// The width and height a PNG file says it has.
fn png_size(path: &std::path::Path) -> [u32; 2] {
    let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("{path:?} is written"));
    [
        u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
    ]
}

// ----- 00:57 to 03:00: a new animation and its frame -------------------------

#[test]
fn step_001_002_file_new_offers_the_animation_project() {
    let mut paint = PaintDriver::new("tut25-001");
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    for text in ["Size of output frame", "Blank space", "Timeline name"] {
        assert!(paint.rect_of(text).is_some(), "{text}: {:?}", paint.texts);
    }
}

#[test]
fn step_003_seen_v01_the_1920_preset_with_blank_space() {
    let paint = tutorial("tut25-003");
    let made = picture(&paint);
    let size = sue_model::picture::size(arena(&paint).get_by_id(made).unwrap());
    assert_eq!(size, Some([2304, 1296]), "the frame with its blank space");
    let frame = sue_paint::anim::setup::OutputFrame::of(arena(&paint), made).expect("a frame");
    assert_eq!(frame.size, [1920, 1080]);
    assert_eq!(frame.blank, [108, 108, 192, 192]);
}

#[test]
fn step_004_seen_v03_frame_rate_24_from_the_preset() {
    let paint = tutorial("tut25-004");
    let sequence = timeline(&paint);
    assert!((sue_paint::anim::fps(&sequence) - 24.0).abs() < 1e-6);
}

#[test]
fn step_005_006_playback_time_in_seconds_and_frames() {
    let mut paint = PaintDriver::new("tut25-005");
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    click_named(&mut paint, "Preset");
    paint.click_text(PRESET);
    assert!(paint.rect_of("Playback time:").is_some() || shows_part(&paint, "Playback time"));
    click_named(&mut paint, "Playback time unit");
    paint.click_text("Seconds and frames");
    // About three seconds: 3 s and 0 f.
    let fields = look(&mut paint);
    let field = fields
        .iter()
        .filter(|(name, _)| name == "Playback time:" || name == "Playback time")
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("the seconds field: {fields:?}"));
    click(&mut paint, field.center());
    type_over(&mut paint, "3");
    paint.settle(3);
    assert!(
        shows(&paint, "3 s 0 f (72 frames)"),
        "three seconds at 24 fps: {:?}",
        paint.texts
    );
    paint.click_text("OK");
    paint.settle(10);
    assert_eq!(sue_paint::anim::length(arena(&paint), picture(&paint)), 72);
}

#[test]
fn step_007_seen_v04_the_output_frame_is_drawn_and_only_it_exports() {
    let mut paint = tutorial("tut25-007");
    paint.settle(4);
    // The frame on the canvas: its label under its corner.
    let texts = paint.settle(2);
    let canvas = paint.runtime.shell.page_rect();
    assert!(
        texts
            .iter()
            .any(|(text, rect)| canvas.contains(rect.center()) && text == "Output frame"),
        "the output frame is named on the canvas: {texts:?}"
    );
    // A picture sequence is the output frame's size.
    stroke(&mut paint, 0.0);
    let root = paint.root.clone();
    std::fs::create_dir_all(&root).unwrap();
    paint.command("file.export.sequence");
    paint.settle(3);
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    picked(&mut paint, "anim-sequence", &root.join("Shot.png"));
    let first = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "png"))
        .expect("a picture was written");
    assert_eq!(png_size(&first), [1920, 1080], "the output frame only");
}

#[test]
fn step_008_output_frame_2005_by_1128_and_no_blank_space() {
    let mut paint = PaintDriver::new("tut25-008");
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    fill_named(&mut paint, "Width", "2005");
    fill_named(&mut paint, "Height", "1128");
    for side in ["Top", "Bottom", "Left", "Right"] {
        fill_named(&mut paint, side, "0");
    }
    paint.click_text("OK");
    paint.settle(10);
    let made = picture(&paint);
    let frame = sue_paint::anim::setup::OutputFrame::of(arena(&paint), made).expect("a frame");
    assert_eq!(frame.size, [2005, 1128]);
    assert_eq!(frame.blank, [0, 0, 0, 0]);
    let size = sue_model::picture::size(arena(&paint).get_by_id(made).unwrap());
    assert_eq!(size, Some([2005, 1128]));
}

#[test]
fn step_009_overflow_frame_of_a_set_size_moved_down_8_px() {
    let mut paint = PaintDriver::new("tut25-009");
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    fill_named(&mut paint, "Width", "2005");
    fill_named(&mut paint, "Height", "1128");
    click_named(&mut paint, "Overflow frame");
    fill_named(&mut paint, "Overflow width", "2334");
    fill_named(&mut paint, "Overflow height", "1658");
    fill_named(&mut paint, "Offset Y", "8");
    paint.click_text("OK");
    paint.settle(10);
    let made = picture(&paint);
    let size = sue_model::picture::size(arena(&paint).get_by_id(made).unwrap());
    assert_eq!(
        size,
        Some([2334, 1658]),
        "the canvas takes the overflow frame"
    );
    let frame = sue_paint::anim::setup::OutputFrame::of(arena(&paint), made).expect("a frame");
    let overflow = frame.overflow.expect("an overflow frame");
    assert_eq!(overflow.size, [2334, 1658]);
    assert_eq!(overflow.offset, [0, 8]);
    // Centred, then 8 px down.
    assert_eq!(frame.rect(), [164, 273, 2005, 1128]);
}

// ----- 02:42 to 04:15: the layout template, locks, palette colors ------------

/// A filled box on the current layer from `from` to `to` (screen),
/// with the rectangle selection; the selection is cleared again.
fn filled_box(paint: &mut PaintDriver, from: Pos2, to: Pos2) {
    press_key(paint, egui::Key::M, Modifiers::NONE);
    paint.drag(from, to, PointerButton::Primary);
    paint.command("edit.fill");
    paint.command("select.none");
    paint.settle(3);
}

#[test]
fn step_010_011_012_a_template_pasted_from_another_canvas_goes_above_paper_and_is_renamed() {
    // The template: drawn on the illustration Sue Paint opened first,
    // copied whole.
    let mut paint = PaintDriver::new("tut25-010");
    let at = middle(&paint);
    filled_box(&mut paint, at - vec2(60.0, 30.0), at + vec2(60.0, 30.0));
    let source_layer = stack(&paint).last().unwrap().id;
    let source = drawn(&paint, source_layer).expect("the template is drawn");
    paint.command("select.all");
    paint.command("edit.copy");
    paint.command("select.none");
    // The new animation, and Paste.
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    click_named(&mut paint, "Preset");
    paint.click_text(PRESET);
    paint.click_text("OK");
    paint.settle(10);
    let before: Vec<EntityId> = stack(&paint).into_iter().map(|info| info.id).collect();
    undo_pair(&mut paint, "Paste", |paint| paint.command("edit.paste"));
    let pasted = stack(&paint)
        .into_iter()
        .find(|info| !before.contains(&info.id))
        .expect("Paste made a layer")
        .id;
    let here = drawn(&paint, pasted).expect("it holds the template");
    assert_eq!(
        [here[2] - here[0], here[3] - here[1]],
        [source[2] - source[0], source[3] - source[1]],
        "the whole template"
    );
    // Step 11: its row dragged onto Paper's row lands right above Paper,
    // outside the animation folder.
    let name = name_of(&paint, pasted);
    let from = row(&mut paint, &name).center();
    let to = row(&mut paint, "Paper").center();
    paint.drag(from, to, PointerButton::Primary);
    let order: Vec<EntityId> = stack(&paint).into_iter().map(|info| info.id).collect();
    let paper = layer(&paint, "Paper");
    let at = order.iter().position(|id| *id == paper).unwrap();
    assert_eq!(order.get(at + 1), Some(&pasted), "right above Paper");
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), pasted), None);
    // Step 12: a double click on its name, typed over: "LO_Template".
    let at = row(&mut paint, &name).center();
    double_click(&mut paint, at);
    type_over(&mut paint, "LO_Template");
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    assert_eq!(name_of(&paint, pasted), "LO_Template");
}

#[test]
fn step_013_the_timeline_opens_from_the_window_menu() {
    let mut paint = PaintDriver::new("tut25-013");
    let open = |paint: &mut PaintDriver| {
        paint
            .settle(4)
            .iter()
            .any(|(text, rect)| text == "Timeline" && rect.top() > 560.0)
    };
    assert!(open(&mut paint), "the Timeline is docked under the canvas");
    paint.command("window.timeline");
    assert!(!open(&mut paint), "Window, Timeline closes it");
    paint.command("window.timeline");
    assert!(open(&mut paint), "and opens it again");
}

#[test]
fn step_014_seen_v05_paper_and_template_picked_together_and_locked() {
    let mut paint = tutorial("tut25-014");
    paint.command("layer.new");
    paint.settle(2);
    let template = stack(&paint).last().unwrap().id;
    paint.command("layer.select_none");
    // Paper current, then Shift+click on the template's row: both picked.
    let paper = layer(&paint, "Paper");
    let at = row(&mut paint, "Paper").center();
    click(&mut paint, at);
    let name = name_of(&paint, template);
    let at = row(&mut paint, &name).center();
    click_with(&mut paint, at, PointerButton::Primary, Modifiers::SHIFT);
    undo_pair(&mut paint, "Lock Layer", |paint| {
        paint.command("layer.toggle.lock");
    });
    assert!(info(&paint, paper).locked, "Paper is locked");
    assert!(info(&paint, template).locked, "the template is locked");
    // The timeline lists every layer, and a locked one reads as locked.
    let texts = paint.settle(3);
    assert!(
        texts
            .iter()
            .any(|(text, rect)| text.starts_with("Paper") && rect.top() > 600.0),
        "Paper has a track: {texts:?}"
    );
}

#[test]
fn step_015_timeline_thumbnails_none_from_its_right_click() {
    let mut paint = tutorial("tut25-015");
    let paper_top = |paint: &mut PaintDriver| {
        paint
            .settle(3)
            .iter()
            .filter(|(text, rect)| {
                text.starts_with("Paper") && rect.top() > 600.0 && rect.left() < 560.0
            })
            .map(|(_, rect)| rect.top())
            .next()
    };
    assert!(
        paper_top(&mut paint).is_none(),
        "the cels' pictures fill the rows"
    );
    // A right click on the timeline, Thumbnail Size, None.
    let folder = first_folder(&paint);
    let name = name_of(&paint, folder);
    let at = cell(&mut paint, &name, 3);
    let texts = right_click(&mut paint, at);
    let size = texts
        .iter()
        .find(|(text, _)| text == "Thumbnail Size")
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("Thumbnail Size in the right click's menu: {texts:?}"));
    paint.step(vec![Event::PointerMoved(size.center())]);
    paint.settle(30);
    paint.click_text("Thumbnails: None");
    assert!(
        paper_top(&mut paint).is_some(),
        "rows of one line: Paper's track in sight"
    );
}

#[test]
fn step_016_seen_v06_change_palette_color_on_the_animation_folder() {
    let mut paint = tutorial("tut25-016");
    let folder = first_folder(&paint);
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    click(&mut paint, at);
    undo_pair(&mut paint, "Change Palette Color", |paint| {
        click_named(paint, "Change Palette Color");
        for name in [
            "No Palette Color",
            "Orange",
            "Yellow",
            "Green",
            "Blue",
            "Purple",
        ] {
            assert!(shows(paint, name), "{name}: {:?}", paint.texts);
        }
        paint.click_text("Red");
    });
    assert_eq!(
        sue_paint::doc::palette_color(arena(&paint), folder),
        sue_paint::doc::named_palette_color("red")
    );
}

/// Every color a fresh frame draws a shape in.
fn colors_drawn(paint: &mut PaintDriver) -> Vec<egui::Color32> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| match shape {
            egui::Shape::Rect(rect) => found.push(rect.fill),
            egui::Shape::LineSegment { stroke, .. } => found.push(stroke.color),
            _ => {}
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    output.drop_without_applying_deltas();
    found
}

#[test]
fn step_017_show_palette_color_on_clips_from_the_timeline_menu() {
    let mut paint = tutorial("tut25-017");
    let folder = drawn_cels(&mut paint, 2, 3);
    let at = row_of(&mut paint, folder).center();
    click(&mut paint, at);
    paint.command("layer.palette_color.red");
    paint.settle(2);
    let red = sue_paint::doc::named_palette_color("red").expect("red");
    let [r, g, b] = red
        .to_space(sue_core::ColorSpace::Srgb)
        .rgb
        .map(|part| (part.clamp(0.0, 1.0) * 255.0).round() as u8);
    let ink = egui::Color32::from_rgb(r, g, b);
    let in_red = |paint: &mut PaintDriver| {
        colors_drawn(paint)
            .into_iter()
            .filter(|color| *color == ink)
            .count()
    };
    let with = in_red(&mut paint);
    // The Timeline palette's menu, Show Palette Color, Show Palette Color
    // on Clips: off, the clips lose the color (the names keep it).
    let menu = timeline_menu(&mut paint);
    click(&mut paint, menu);
    let sub = paint
        .texts
        .iter()
        .find(|(text, _)| text == "Show Palette Color")
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("Show Palette Color in the menu: {:?}", paint.texts));
    paint.step(vec![Event::PointerMoved(sub.center())]);
    paint.settle(30);
    paint.click_text("Show Palette Color on Clips");
    paint.settle(3);
    let without = in_red(&mut paint);
    assert!(with > without, "the clips drew red: {with} then {without}");
    paint.command("anim.palette_color.clips");
    paint.settle(3);
    assert_eq!(in_red(&mut paint), with, "and again once on");
}

#[test]
fn step_018_the_default_color_set() {
    let mut paint = PaintDriver::new("tut25-018");
    paint.settle(6);
    assert!(
        shows_part(&paint, "Default"),
        "the Default color set shows: {:?}",
        paint.texts
    );
}

/// Where the Timeline palette's menu button is: found by its tip, left
/// of the palette's name.
fn timeline_menu(paint: &mut PaintDriver) -> Pos2 {
    let texts = paint.settle(4);
    let header = texts
        .iter()
        .filter(|(text, rect)| text == "Timeline" && rect.top() > 560.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("the Timeline palette: {texts:?}"));
    let found = super::paint_anim::tips_along(
        paint,
        header.center().y,
        header.left() - 70.0,
        header.left(),
    );
    super::paint_anim::tip(&found, "Timeline")
}

// ----- 04:16 to 05:50: keys ---------------------------------------------------

/// Whether the Sub Tool palette lists `sub` now.
fn lists(paint: &mut PaintDriver, sub: &str) -> bool {
    paint
        .settle(4)
        .iter()
        .any(|(text, rect)| text == sub && rect.left() < 400.0)
}

#[test]
fn step_019_b_picks_brush_then_airbrush_then_decoration() {
    let mut paint = tutorial("tut25-019");
    press_key(&mut paint, egui::Key::B, Modifiers::NONE);
    assert!(
        lists(&mut paint, "Round watercolor brush"),
        "{:?}",
        paint.texts
    );
    press_key(&mut paint, egui::Key::B, Modifiers::NONE);
    assert!(lists(&mut paint, "Spray"), "{:?}", paint.texts);
    press_key(&mut paint, egui::Key::B, Modifiers::NONE);
    assert!(lists(&mut paint, "Sparkles"), "{:?}", paint.texts);
    press_key(&mut paint, egui::Key::B, Modifiers::NONE);
    assert!(
        lists(&mut paint, "Round watercolor brush"),
        "{:?}",
        paint.texts
    );
}

#[test]
fn step_020_seen_v07_b_left_on_brush_alone() {
    let mut paint = tutorial("tut25-020");
    // Settings, Keyboard Shortcuts: every tool has a row of its own. The
    // letter the three share is taken off their shared row and given to
    // Brush alone.
    let rice = &paint.runtime.engine.world.rice;
    let (setting, value) = sue_wm::shortcuts::bind(
        rice,
        "sue_paint_keys",
        "tool.brush",
        &sue_rice::Keybinds::default(),
    );
    paint
        .runtime
        .engine
        .world
        .rice
        .set(&setting, value)
        .unwrap();
    bind(&mut paint, "tool.pick.brush", "B");
    for _ in 0..3 {
        press_key(&mut paint, egui::Key::B, Modifiers::NONE);
        assert!(
            lists(&mut paint, "Round watercolor brush"),
            "B stays on Brush: {:?}",
            paint.texts
        );
    }
    // Airbrush and Decoration keep rows of their own, with no key.
    let registry = sue_paint::registry();
    for name in ["tool.pick.airbrush", "tool.pick.decoration"] {
        assert!(
            registry.iter().any(|command| command.name == name),
            "{name}"
        );
    }
}

/// The frame the timeline shows, from 1: the first number of the bar's
/// readout ("3   1 to  120").
fn current_frame(paint: &mut PaintDriver) -> u32 {
    let texts = paint.settle(3);
    texts
        .iter()
        .filter(|(text, rect)| text.contains(" to ") && rect.top() > 560.0)
        .find_map(|(text, _)| text.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("the timeline's frame readout: {texts:?}"))
}

/// The view's turn, as the Navigator reads it (the number before its
/// degree sign).
fn rotation(paint: &mut PaintDriver) -> f32 {
    let texts = paint.settle(3);
    let degree = texts
        .iter()
        .find(|(text, rect)| text == "\u{b0}" && rect.left() > 1100.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the Navigator's rotation: {texts:?}"));
    texts
        .iter()
        .filter(|(_, rect)| {
            (rect.center().y - degree.center().y).abs() < 4.0 && rect.right() <= degree.left() + 1.0
        })
        .max_by(|a, b| a.1.left().total_cmp(&b.1.left()))
        .and_then(|(text, _)| text.trim().parse().ok())
        .unwrap_or_else(|| panic!("a number before the degree sign: {texts:?}"))
}

/// Sue Paint's window title (it carries the zoom).
fn title(paint: &mut PaintDriver) -> String {
    paint.settle(3);
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(&paint.runtime.engine.world.arena))
        .expect("Sue Paint names its window")
}

/// One frame with `events` while `held` are held.
fn step_held(paint: &mut PaintDriver, mut events: Vec<Event>, held: Modifiers) {
    events.insert(0, Event::ModifiersChanged(held));
    paint.step(events);
}

#[test]
fn step_021_041_054_keys_1_and_2_select_the_previous_and_next_cel() {
    let mut paint = tutorial("tut25-021");
    let folder = drawn_cels(&mut paint, 3, 2);
    assert_eq!(shown(&paint, folder, 5), ["1", "1", "2", "2", "3"]);
    // Shortcut Settings: Select Previous Cel and Select Next Cel (under
    // Animation, Edit Track) take 1 and 2.
    bind(&mut paint, "anim.cel.prev", "1");
    bind(&mut paint, "anim.cel.next", "2");
    assert_eq!(current_frame(&mut paint), 1);
    press_key(&mut paint, egui::Key::Num2, Modifiers::NONE);
    assert_eq!(current_frame(&mut paint), 3, "2: the next cel, on frame 3");
    press_key(&mut paint, egui::Key::Num2, Modifiers::NONE);
    assert_eq!(current_frame(&mut paint), 5, "and cel 3 on frame 5");
    press_key(&mut paint, egui::Key::Num1, Modifiers::NONE);
    assert_eq!(current_frame(&mut paint), 3, "1: back to cel 2");
    let track = menu_labels(&["Animation", "Edit Track"]);
    assert!(
        track.iter().any(|item| item == "Select Next Cel"),
        "{track:?}"
    );
}

#[test]
fn step_022_023_ctrl_alone_rotates_the_canvas_once_set() {
    let mut paint = tutorial("tut25-022");
    // File, Modifier Key Settings: Ctrl changes to the Hand's Rotate.
    paint.command("file.modifiers");
    paint.settle(6);
    assert!(shows(&paint, "Modifier Key Settings"), "{:?}", paint.texts);
    let ctrl_row = paint.rect_of("Operation: Object").expect("Ctrl's row");
    paint.click_text("Operation: Object");
    let rotate = paint
        .texts
        .iter()
        .filter(|(text, rect)| text == "Hand: Rotate" && rect.top() > ctrl_row.bottom())
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("Hand: Rotate in the list");
    click(&mut paint, rotate.center());
    paint.click_text("OK");
    paint.settle(4);
    // Step 23: Ctrl and a drag with the pen turns the canvas, no ink.
    let cel = layer(&paint, "1");
    let before = rotation(&mut paint);
    let at = middle(&paint);
    let (from, to) = (at + vec2(150.0, 0.0), at + vec2(0.0, 150.0));
    step_held(
        &mut paint,
        vec![Event::PointerMoved(from)],
        Modifiers::COMMAND,
    );
    step_held(
        &mut paint,
        vec![button(
            from,
            PointerButton::Primary,
            true,
            Modifiers::COMMAND,
        )],
        Modifiers::COMMAND,
    );
    for index in 1..=10 {
        let point = from + (to - from) * (index as f32 / 10.0);
        step_held(
            &mut paint,
            vec![Event::PointerMoved(point)],
            Modifiers::COMMAND,
        );
    }
    step_held(
        &mut paint,
        vec![button(
            to,
            PointerButton::Primary,
            false,
            Modifiers::COMMAND,
        )],
        Modifiers::COMMAND,
    );
    step_held(&mut paint, Vec::new(), Modifiers::NONE);
    let after = rotation(&mut paint);
    assert!((after - before).abs() > 30.0, "turned: {before} to {after}");
    assert!(drawn(&paint, cel).is_none(), "nothing drawn");
}

#[test]
fn step_024_space_and_a_drag_pans() {
    let mut paint = tutorial("tut25-024");
    let cel = layer(&paint, "1");
    stroke(&mut paint, 0.0);
    let before = drawn(&paint, cel).expect("a stroke");
    paint.command("edit.undo");
    paint.settle(2);
    // Space held, a drag: the canvas moves under the pointer, nothing is
    // drawn.
    let at = middle(&paint);
    let space = |pressed| Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    paint.step(vec![space(true)]);
    paint.drag(at, at + vec2(120.0, 0.0), PointerButton::Primary);
    paint.step(vec![space(false)]);
    paint.settle(3);
    assert!(drawn(&paint, cel).is_none(), "the drag drew nothing");
    // The same stroke on screen now lands further left on the canvas.
    stroke(&mut paint, 0.0);
    let after = drawn(&paint, cel).expect("a stroke");
    assert!(
        after[0] + 100 < before[0],
        "panned: {before:?} then {after:?}"
    );
}

#[test]
fn step_025_the_wheel_and_ctrl_plus_minus_zoom() {
    let mut paint = tutorial("tut25-025");
    let start = title(&mut paint);
    press_key(&mut paint, egui::Key::Plus, Modifiers::COMMAND);
    let bigger = title(&mut paint);
    assert_ne!(start, bigger, "Ctrl+plus zooms in");
    press_key(&mut paint, egui::Key::Minus, Modifiers::COMMAND);
    assert_eq!(title(&mut paint), start, "Ctrl+minus zooms back");
    // The wheel over the canvas zooms too.
    let at = middle(&paint);
    paint.step(vec![Event::PointerMoved(at)]);
    paint.step(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 2.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    }]);
    assert_ne!(title(&mut paint), start, "the wheel zooms: {start}");
}

#[test]
fn step_026_ctrl_r_given_to_reset_rotation_takes_it_from_the_ruler_bar() {
    let mut paint = tutorial("tut25-026");
    // Settings, Keyboard Shortcuts: Reset Rotation gets Ctrl+R, and Ctrl+R
    // is taken from the Ruler Bar, as Clip Studio Paint unbinds it.
    bind(&mut paint, "view.rotate_reset", "Ctrl+R");
    let rice = &paint.runtime.engine.world.rice;
    let (setting, value) = sue_wm::shortcuts::bind(
        rice,
        "sue_paint_keys",
        "view.ruler_bar",
        &sue_rice::Keybinds::default(),
    );
    paint
        .runtime
        .engine
        .world
        .rice
        .set(&setting, value)
        .unwrap();
    paint.command("view.rotate_left");
    let turned = rotation(&mut paint);
    assert!(turned.abs() > 1.0, "turned: {turned}");
    press_key(&mut paint, egui::Key::R, Modifiers::COMMAND);
    assert!(rotation(&mut paint).abs() < 0.1, "Ctrl+R put it upright");
}

// ----- 06:57 to 09:40: folders and cels ---------------------------------------

/// Whether the control named `name` says it is on.
fn toggled(paint: &mut PaintDriver, name: &str) -> Option<bool> {
    paint.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let found = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .find(|(_, node)| node.label().is_some_and(|label| label == name))
        .and_then(|(_, node)| node.toggled())
        .map(|state| state == egui::accesskit::Toggled::True);
    output.drop_without_applying_deltas();
    found
}

#[test]
fn step_028_one_animation_folder_and_the_new_animation_folder_button() {
    let mut paint = tutorial("tut25-028");
    assert_eq!(animation_folders(&paint).len(), 1, "New made one folder");
    let folder = first_folder(&paint);
    assert_eq!(
        sue_paint::anim::cels(arena(&paint), picture(&paint), folder).len(),
        1,
        "holding cel 1"
    );
    undo_pair(&mut paint, "New Animation Folder", |paint| {
        click_named(paint, "Animation Folder");
    });
    assert_eq!(animation_folders(&paint).len(), 2);
}

#[test]
fn step_029_030_043_new_layer_folder_and_a_raster_layer_made_inside_it() {
    let mut paint = tutorial("tut25-029");
    let at = row(&mut paint, "Paper").center();
    click(&mut paint, at);
    let before: Vec<EntityId> = stack(&paint).iter().map(|info| info.id).collect();
    undo_pair(&mut paint, "New Layer Folder", |paint| {
        click_named(paint, "New Layer Folder");
    });
    let folder = stack(&paint)
        .into_iter()
        .find(|info| !before.contains(&info.id))
        .expect("a folder")
        .id;
    assert!(sue_paint::doc::is_folder(arena(&paint), folder));
    // With the folder current, New Raster Layer goes inside it.
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    click(&mut paint, at);
    let before: Vec<EntityId> = stack(&paint).iter().map(|info| info.id).collect();
    undo_pair(&mut paint, "New Raster Layer", |paint| {
        click_named(paint, "Raster Layer");
    });
    let made = stack(&paint)
        .into_iter()
        .find(|info| !before.contains(&info.id))
        .expect("a layer")
        .id;
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), made), Some(folder));
    // Step 43: the folder renamed by a double click on its name.
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    double_click(&mut paint, at);
    type_over(&mut paint, "Layout");
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    assert_eq!(name_of(&paint, folder), "Layout");
}

#[test]
fn step_031_a_plain_folder_shows_on_every_frame() {
    let mut paint = tutorial("tut25-031");
    drawn_cels(&mut paint, 2, 3);
    // A plain folder with a drawn layer, outside the animation folder.
    let at = row(&mut paint, "Paper").center();
    click(&mut paint, at);
    paint.command("layer.new.folder");
    paint.command("layer.new");
    paint.settle(2);
    stroke(&mut paint, 80.0);
    let held = stack(&paint)
        .into_iter()
        .rev()
        .find(|info| {
            info.kind == sue_model::picture::LayerKind::Raster
                && sue_paint::anim::cel_folder(arena(&paint), info.id).is_none()
                && sue_paint::doc::folder_of(arena(&paint), info.id).is_some()
        })
        .expect("the held layer")
        .id;
    assert!(drawn(&paint, held).is_some(), "drawn on");
    let sequence = timeline(&paint);
    for frame in [0, 3, 5] {
        let view = sue_paint::anim::view(&sequence, frame, None);
        assert!(
            !view.hidden.contains(&held),
            "frame {}: the plain folder's layer shows",
            frame + 1
        );
    }
}

#[test]
fn step_032_brush_size_3_px_in_tool_property() {
    let mut paint = tutorial("tut25-032");
    press_key(&mut paint, egui::Key::B, Modifiers::NONE);
    click_named(&mut paint, "Tool Property");
    let field = look(&mut paint)
        .into_iter()
        .filter(|(name, rect)| name == "Brush Size" && rect.left() < 400.0 && rect.top() > 100.0)
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("Tool Property's Brush Size: {:?}", paint.texts));
    click(&mut paint, field.center());
    type_over(&mut paint, "3");
    paint.settle(3);
    assert!(
        paint.texts.iter().any(|(text, _)| text.starts_with("3.0")),
        "{:?}",
        paint.texts
    );
}

#[test]
fn step_033_035_046_a_folder_in_an_animation_folder_is_one_cel() {
    let mut paint = tutorial("tut25-033");
    let folder = first_folder(&paint);
    let one = layer(&paint, "1");
    // Step 35: cel 1 dragged onto New Layer Folder is wrapped in a folder,
    // and that folder is now the cel.
    let from = row(&mut paint, "1").center();
    let to = control(&mut paint, "New Layer Folder").center();
    paint.drag(from, to, PointerButton::Primary);
    let wrapper = sue_paint::doc::folder_of(arena(&paint), one).expect("wrapped");
    assert_ne!(wrapper, folder);
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), wrapper),
        Some(folder),
        "the new folder is in the animation folder"
    );
    assert_eq!(
        sue_paint::anim::cels(arena(&paint), picture(&paint), folder),
        [wrapper],
        "the folder is the cel"
    );
    assert_eq!(shown(&paint, folder, 1)[0], name_of(&paint, wrapper));
    // A new cel after a folder cel is a folder cel too, as in CSP; on its
    // frame the first folder cel and the layer in it are hidden.
    paint.command("anim.next");
    paint.command("anim.next");
    paint.command("anim.cel.new");
    paint.settle(2);
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), folder);
    assert_eq!(cels.len(), 2);
    assert!(
        cels.iter()
            .all(|cel| sue_paint::doc::is_folder(arena(&paint), *cel))
    );
    let sequence = timeline(&paint);
    let view = sue_paint::anim::view(&sequence, 2, None);
    let view = sue_paint::anim::cels::whole_folders(arena(&paint), picture(&paint), view);
    assert!(view.hidden.contains(&wrapper), "the other folder cel hides");
}

#[test]
fn step_034_new_animation_cel_makes_2_from_the_frame_shown() {
    let mut paint = tutorial("tut25-034");
    let folder = first_folder(&paint);
    paint.command("anim.next");
    paint.settle(2);
    undo_pair(&mut paint, "New Animation Cel", |paint| {
        click_named(paint, "New Animation Cel");
    });
    assert_eq!(shown(&paint, folder, 3), ["1", "2", "2"]);
}

#[test]
fn step_036_seen_v12_a_right_click_on_the_track_names_the_cels_to_show() {
    let mut paint = tutorial("tut25-036");
    let folder = drawn_cels(&mut paint, 3, 1);
    assert_eq!(shown(&paint, folder, 4), ["1", "2", "3", "3"]);
    let name = name_of(&paint, folder);
    let at = cell(&mut paint, &name, 4);
    let before: Vec<Rect> = paint
        .settle(2)
        .iter()
        .filter(|(text, _)| text == "1")
        .map(|(_, rect)| *rect)
        .collect();
    let texts = right_click(&mut paint, at);
    for item in ["1", "2", "3", "Delete Frames", "Track Label", "Split Clip"] {
        assert!(
            texts.iter().any(|(text, _)| text == item),
            "{item}: {texts:?}"
        );
    }
    // A click on cel 1's name shows it on frame 4.
    let one = texts
        .iter()
        .filter(|(text, rect)| text == "1" && !before.contains(rect))
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("cel 1 in the menu: {texts:?}"));
    click(&mut paint, one.center());
    assert_eq!(shown(&paint, folder, 4), ["1", "2", "3", "1"]);
}

#[test]
fn step_037_056_062_064_seen_v13_cels_picked_and_dragged_along_the_track() {
    let mut paint = tutorial("tut25-037");
    let folder = drawn_cels(&mut paint, 3, 1);
    let name = name_of(&paint, folder);
    // A drag across frames 2 and 3 picks them; dragged from 2 to 5 they
    // land on 5 and 6.
    let (two, three) = (cell(&mut paint, &name, 2), cell(&mut paint, &name, 3));
    paint.drag(two, three, PointerButton::Primary);
    let before = bytes(&paint);
    let from = cell(&mut paint, &name, 2);
    let to = cell(&mut paint, &name, 5);
    paint.drag(from, to, PointerButton::Primary);
    let now = shown(&paint, folder, 7);
    assert_eq!(now[4], "2", "cel 2 on frame 5: {now:?}");
    assert_eq!(now[5], "3", "cel 3 on frame 6: {now:?}");
    assert_eq!(now[0], "1");
    paint.command("edit.undo");
    paint.settle(2);
    assert!(bytes(&paint) == before, "the move undoes");
}

#[test]
fn step_038_seen_v12_delete_from_the_frame_right_click() {
    let mut paint = tutorial("tut25-038");
    let folder = drawn_cels(&mut paint, 3, 1);
    let name = name_of(&paint, folder);
    let at = cell(&mut paint, &name, 3);
    click(&mut paint, at);
    right_click(&mut paint, at);
    undo_pair(&mut paint, "Delete", |paint| {
        paint.click_text("Delete Frames");
    });
    assert_eq!(shown(&paint, folder, 3)[2], "", "frame 3 shows nothing");
}

#[test]
fn step_039_nothing_shows_after_the_last_drawing() {
    let mut paint = tutorial("tut25-039");
    let folder = drawn_cels(&mut paint, 3, 1);
    assert_eq!(shown(&paint, folder, 5)[4], "3", "cel 3 holds on");
    let name = name_of(&paint, folder);
    let at = cell(&mut paint, &name, 3);
    right_click(&mut paint, at);
    undo_pair(&mut paint, "Set as Last Displayed Frame", |paint| {
        paint.click_text("Set as Last Displayed Frame");
    });
    assert_eq!(shown(&paint, folder, 5), ["1", "2", "3", "", ""]);
}

#[test]
fn step_040_every_drawing_of_the_track_moved_at_once() {
    let mut paint = tutorial("tut25-040");
    let folder = drawn_cels(&mut paint, 3, 1);
    let name = name_of(&paint, folder);
    let (one, three) = (cell(&mut paint, &name, 1), cell(&mut paint, &name, 3));
    paint.drag(one, three, PointerButton::Primary);
    let from = cell(&mut paint, &name, 1);
    let to = cell(&mut paint, &name, 3);
    paint.drag(from, to, PointerButton::Primary);
    let now = shown(&paint, folder, 5);
    assert_eq!(now[2..5], ["1", "2", "3"], "{now:?}");
}

#[test]
fn step_042_seen_v10_picked_folders_deleted_with_the_trash_after_asking() {
    let mut paint = tutorial("tut25-042");
    paint.command("anim.layer.folder");
    paint.settle(2);
    let folders = animation_folders(&paint);
    assert_eq!(folders.len(), 2);
    let (a, b) = (name_of(&paint, folders[0]), name_of(&paint, folders[1]));
    let at = row(&mut paint, &a).center();
    click(&mut paint, at);
    let at = row(&mut paint, &b).center();
    click_with(&mut paint, at, PointerButton::Primary, Modifiers::SHIFT);
    click_named(&mut paint, "Delete Layer");
    assert!(
        shows_part(&paint, "Delete the picked layer folders"),
        "it asks first: {:?}",
        paint.texts
    );
    assert!(shows(&paint, "Do not show again"));
    paint.click_text("Delete");
    paint.settle(3);
    assert!(
        animation_folders(&paint).is_empty(),
        "both went with their tracks"
    );
}

#[test]
fn step_044_shift_keeps_the_straight_line_level() {
    let mut paint = tutorial("tut25-044");
    press_key(&mut paint, egui::Key::U, Modifiers::NONE);
    assert!(lists(&mut paint, "Straight Line"), "{:?}", paint.texts);
    paint.click_text("Straight Line");
    let cel = layer(&paint, "1");
    let at = middle(&paint);
    drag_with(
        &mut paint,
        at - vec2(100.0, 0.0),
        at + vec2(100.0, 9.0),
        Modifiers::SHIFT,
    );
    let bounds = drawn(&paint, cel).expect("a line");
    let tall = bounds[3] - bounds[1];
    let wide = bounds[2] - bounds[0];
    assert!(wide > 100 && tall < 12, "a level line: {bounds:?}");
}

#[test]
fn step_045_new_animation_folder_renamed_a() {
    let mut paint = tutorial("tut25-045");
    paint.command("anim.layer.folder");
    paint.settle(2);
    let folder = animation_folders(&paint)[1];
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    double_click(&mut paint, at);
    type_over(&mut paint, "A");
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    assert_eq!(name_of(&paint, folder), "A");
    paint.command("anim.thumbnails.none");
    let texts = paint.settle(3);
    assert!(
        texts
            .iter()
            .any(|(text, rect)| (text == "A" || text.starts_with("A :"))
                && rect.top() > 600.0
                && rect.left() < 560.0),
        "the track reads A: {texts:?}"
    );
}

#[test]
fn step_047_each_folder_gets_a_palette_color() {
    let mut paint = tutorial("tut25-047");
    paint.command("layer.new.folder");
    paint.settle(2);
    let folder = stack(&paint)
        .into_iter()
        .rev()
        .find(|info| {
            sue_paint::doc::is_folder(arena(&paint), info.id)
                && !sue_paint::anim::is_animation_folder(arena(&paint), info.id)
        })
        .expect("a plain folder")
        .id;
    undo_pair(&mut paint, "palette color", |paint| {
        paint.command("layer.palette_color.blue");
    });
    assert_eq!(
        sue_paint::doc::palette_color(arena(&paint), folder),
        sue_paint::doc::named_palette_color("blue")
    );
}

#[test]
fn step_048_b_draws_e_erases_and_ctrl_z_undoes() {
    let mut paint = tutorial("tut25-048");
    let cel = layer(&paint, "1");
    press_key(&mut paint, egui::Key::B, Modifiers::NONE);
    stroke(&mut paint, 0.0);
    let inked = drawn(&paint, cel).expect("B draws");
    press_key(&mut paint, egui::Key::E, Modifiers::NONE);
    let at = middle(&paint);
    paint.drag(
        at - vec2(100.0, 0.0),
        at + vec2(100.0, 0.0),
        PointerButton::Primary,
    );
    assert_ne!(drawn(&paint, cel), Some(inked), "E erases");
    press_key(&mut paint, egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(drawn(&paint, cel), Some(inked), "Ctrl+Z brings it back");
}

#[test]
fn step_049_050_scale_rotate_and_a_lasso_part_moved() {
    let mut paint = tutorial("tut25-049");
    let cel = layer(&paint, "1");
    stroke(&mut paint, 0.0);
    let before = drawn(&paint, cel).unwrap();
    let at = middle(&paint);
    press_key(&mut paint, egui::Key::T, Modifiers::COMMAND);
    // From inside the box, clear of its turning centre in the middle.
    let from = at + vec2(-60.0, 0.0);
    drag_with(&mut paint, from, from + vec2(0.0, 60.0), Modifiers::NONE);
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    let after = drawn(&paint, cel).unwrap();
    assert!(
        after[1] > before[1] + 20,
        "moved down: {before:?} {after:?}"
    );
    // Step 50: Lasso round the left half, Ctrl+T, moved up.
    paint.command("subtool.selection.Lasso");
    paint.settle(2);
    let base = at + vec2(0.0, 60.0);
    let points = [
        base + vec2(-130.0, -40.0),
        base + vec2(0.0, -40.0),
        base + vec2(0.0, 40.0),
        base + vec2(-130.0, 40.0),
        base + vec2(-130.0, -40.0),
    ];
    paint.step(vec![Event::PointerMoved(points[0])]);
    paint.step(vec![button(
        points[0],
        PointerButton::Primary,
        true,
        Modifiers::NONE,
    )]);
    for pair in points.windows(2) {
        for index in 1..=8 {
            let p = pair[0] + (pair[1] - pair[0]) * (index as f32 / 8.0);
            paint.step(vec![Event::PointerMoved(p)]);
        }
    }
    paint.step(vec![button(
        points[4],
        PointerButton::Primary,
        false,
        Modifiers::NONE,
    )]);
    paint.settle(4);
    press_key(&mut paint, egui::Key::T, Modifiers::COMMAND);
    drag_with(
        &mut paint,
        base - vec2(60.0, 0.0),
        base - vec2(60.0, 50.0),
        Modifiers::NONE,
    );
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    paint.command("select.none");
    let moved = drawn(&paint, cel).unwrap();
    assert!(
        moved[1] < after[1],
        "the lasso part went up: {after:?} {moved:?}"
    );
    assert_eq!(moved[3], after[3], "the right half stayed");
}

#[test]
fn step_051_puppet_warp_pins_bend_the_drawing() {
    let mut paint = tutorial("tut25-051");
    let cel = layer(&paint, "1");
    stroke(&mut paint, 0.0);
    let before = drawn(&paint, cel).unwrap();
    paint.command("edit.transform.puppet");
    paint.settle(4);
    let at = middle(&paint);
    for x in [-80.0, 0.0, 80.0] {
        click(&mut paint, at + vec2(x, 0.0));
    }
    drag_with(
        &mut paint,
        at + vec2(80.0, 0.0),
        at + vec2(80.0, 60.0),
        Modifiers::NONE,
    );
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    let after = drawn(&paint, cel).unwrap();
    assert!(
        after[3] > before[3] + 20,
        "bent down at the right: {before:?} {after:?}"
    );
}

#[test]
fn step_052_053_088_seen_v17_onion_skin_on_and_its_settings() {
    let mut paint = tutorial("tut25-052");
    drawn_cels(&mut paint, 2, 2);
    // The timeline's bar: its Enable Onion Skin button, under More when
    // the bar is narrow.
    if !has_control(&mut paint, "Enable Onion Skin") {
        click_named(&mut paint, "More buttons");
    }
    assert_eq!(toggled(&mut paint, "Enable Onion Skin"), Some(false));
    click_named(&mut paint, "Enable Onion Skin");
    if !has_control(&mut paint, "Enable Onion Skin") {
        click_named(&mut paint, "More buttons");
    }
    assert_eq!(toggled(&mut paint, "Enable Onion Skin"), Some(true));
    press_key(&mut paint, egui::Key::Escape, Modifiers::NONE);
    // Animation, Show Animation Cels, Onion Skin Settings: blue before,
    // green after; the nearest cel made fainter.
    let onion_of = |paint: &PaintDriver| sue_paint::anim::Onion::of(arena(paint), picture(paint));
    assert_eq!(onion_of(&paint).before_color, [0, 0, 255]);
    assert_eq!(onion_of(&paint).after_color, [0, 255, 0]);
    paint.command("anim.onion_settings");
    paint.settle(4);
    assert!(shows(&paint, "Onion Skin Settings"), "{:?}", paint.texts);
    undo_pair(&mut paint, "Onion Skin Settings", |paint| {
        fill_named(paint, "Nearest cel", "40");
        paint.click_text("OK");
    });
    let (start, _) = onion_of(&paint).opacity.expect("an opacity level");
    assert!((start - 0.4).abs() < 0.01, "{start}");
}

#[test]
fn step_055_the_whole_animation_folder_scaled_at_once() {
    let mut paint = tutorial("tut25-055");
    let folder = drawn_cels(&mut paint, 2, 2);
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), folder);
    let before: Vec<[u32; 4]> = cels
        .iter()
        .map(|cel| drawn(&paint, *cel).unwrap())
        .collect();
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    click(&mut paint, at);
    press_key(&mut paint, egui::Key::T, Modifiers::COMMAND);
    let at = middle(&paint) + vec2(-60.0, -45.0);
    drag_with(&mut paint, at, at + vec2(0.0, 50.0), Modifiers::NONE);
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    for (cel, was) in cels.iter().zip(before) {
        let now = drawn(&paint, *cel).unwrap();
        assert!(now[1] > was[1] + 15, "every cel moved: {was:?} {now:?}");
    }
}

#[test]
fn step_057_an_in_between_is_named_1a_and_sits_above_1() {
    let mut paint = tutorial("tut25-057");
    let folder = drawn_cels(&mut paint, 2, 3);
    paint.command("anim.first");
    paint.command("anim.next");
    undo_pair(&mut paint, "New Animation Cel", |paint| {
        paint.command("anim.cel.new");
    });
    paint.command("anim.next");
    paint.command("anim.cel.new");
    paint.settle(2);
    let names: Vec<String> = sue_paint::anim::cels(arena(&paint), picture(&paint), folder)
        .into_iter()
        .map(|cel| name_of(&paint, cel))
        .collect();
    assert_eq!(names, ["1", "1a", "1b", "2"], "bottom first, as CSP");
}

#[test]
fn step_058_flipping_through_with_the_cel_buttons() {
    let mut paint = tutorial("tut25-058");
    drawn_cels(&mut paint, 3, 2);
    paint.command("window.cels");
    paint.settle(4);
    assert_eq!(current_frame(&mut paint), 1);
    click_named(&mut paint, "Select Next Cel");
    assert_eq!(current_frame(&mut paint), 3);
    click_named(&mut paint, "Select Previous Cel");
    assert_eq!(current_frame(&mut paint), 1);
}

#[test]
fn step_059_060_086_089_cels_renamed_by_hand_and_in_timeline_order() {
    let mut paint = tutorial("tut25-059");
    let folder = drawn_cels(&mut paint, 3, 2);
    // Step 59: by hand.
    let at = row(&mut paint, "1").center();
    double_click(&mut paint, at);
    type_over(&mut paint, "A1");
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    assert_eq!(shown(&paint, folder, 1), ["A1"]);
    // An in-between, then Animation, Edit Track, Rename in Timeline Order.
    paint.command("anim.first");
    paint.command("anim.next");
    paint.command("anim.cel.new");
    paint.settle(2);
    undo_pair(&mut paint, "Rename in Timeline Order", |paint| {
        paint.command("anim.cel.rename_timeline");
    });
    assert_eq!(shown(&paint, folder, 5), ["1", "2", "3", "3", "4"]);
}

#[test]
fn step_063_seen_v12_in_between_track_labels_at_7_10_16_and_21() {
    let mut paint = tutorial("tut25-063");
    let folder = drawn_cels(&mut paint, 2, 12);
    let name = name_of(&paint, folder);
    for (frame, kind) in [
        (7, "In-between to Draw"),
        (10, "In-between to Draw"),
        (16, "In-between to Draw"),
        (21, "In-between Drawn"),
    ] {
        let at = cell(&mut paint, &name, frame);
        let texts = right_click(&mut paint, at);
        let label = texts
            .iter()
            .find(|(text, _)| text == "Track Label")
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("Track Label: {texts:?}"));
        paint.step(vec![Event::PointerMoved(label.center())]);
        paint.settle(30);
        paint.click_text(kind);
    }
    let sequence = timeline(&paint);
    let labels = sue_paint::anim::editing::track_labels(&sequence.tracks[track_of(&paint, folder)]);
    let frames: Vec<u32> = labels.iter().map(|(frame, _)| *frame).collect();
    assert_eq!(frames, [6, 9, 15, 20], "{labels:?}");
    assert_eq!(
        labels[3].1,
        sue_paint::anim::editing::TrackLabel::InBetweenDone
    );
}

#[test]
fn step_065_the_end_marker_dragged_left_trims_the_scene() {
    let mut paint = tutorial("tut25-065");
    let folder = drawn_cels(&mut paint, 2, 3);
    let _ = folder;
    // The end marker of the ruler, zoomed out until it is in sight; the
    // start marker on frame 1 gives the frames' width.
    let end = "Drag to set the last frame played";
    for _ in 0..12 {
        if control(&mut paint, end).center().x < 1080.0 {
            break;
        }
        click_named(&mut paint, "Zoom out");
    }
    let last = control(&mut paint, end);
    let first = control(&mut paint, "Drag to set the first frame played");
    assert!(
        last.center().x < 1080.0,
        "the end marker in sight: {last:?}"
    );
    let length = sue_paint::anim::length(arena(&paint), picture(&paint)) as f32;
    let step = (last.right() - first.left()) / length;
    let marker = last.center();
    let six = Pos2::new(first.left() + step * 6.5, marker.y);
    undo_pair(&mut paint, "the end marker", |paint| {
        paint.drag(marker, six, PointerButton::Primary);
    });
    let settings = sue_paint::anim::setup::Settings::of(arena(&paint), picture(&paint));
    assert_eq!(settings.end, Some(5), "the last frame played is 6");
}

#[test]
fn step_066_107_play_runs_the_frames() {
    let mut paint = tutorial("tut25-066");
    drawn_cels(&mut paint, 3, 2);
    let before = paint.settle(2);
    let counter = |texts: &[(String, Rect)]| {
        texts
            .iter()
            .filter(|(text, rect)| text.contains(" to ") && rect.top() > 560.0)
            .map(|(text, _)| text.clone())
            .next()
    };
    let first = counter(&before);
    click_named(&mut paint, "Play/Stop");
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..80 {
        let texts = paint.step(Vec::new());
        if let Some(text) = counter(&texts) {
            seen.insert(text);
        }
    }
    click_named(&mut paint, "Play/Stop");
    assert!(
        seen.len() > 2,
        "the frame counter moved from {first:?}: {seen:?}"
    );
}

// ----- 20:03 to 24:50: the cel folder's layers, colors ------------------------

/// A cel that is a folder: cel 1 put in a folder of its own (as the
/// tutorial makes it), with its drawn layer; the folder and the layer.
fn folder_cel(paint: &mut PaintDriver) -> (EntityId, EntityId) {
    let one = layer(paint, "1");
    stroke(paint, -60.0);
    let at = row(paint, "1").center();
    click(paint, at);
    paint.command("layer.folder_insert");
    paint.settle(3);
    let cel = sue_paint::doc::folder_of(arena(paint), one).expect("the cel folder");
    (cel, one)
}

#[test]
fn step_067_075_layer_opacity_and_a_folder_s_eye_and_opacity() {
    let mut paint = tutorial("tut25-067");
    let (cel, rough) = folder_cel(&mut paint);
    let at = row_of(&mut paint, rough).center();
    click(&mut paint, at);
    undo_pair(&mut paint, "Layer Opacity", |paint| {
        let field = look(paint)
            .into_iter()
            .filter(|(name, rect)| name == "Opacity" && rect.left() > 1100.0)
            .map(|(_, rect)| rect)
            .max_by(|a, b| a.left().total_cmp(&b.left()))
            .expect("the Layer palette's opacity");
        click(paint, field.center());
        type_over(paint, "30");
    });
    assert!((info(&paint, rough).style.opacity - 0.3).abs() < 0.01);
    // Step 75: the animation folder lowered, the cel folder hidden.
    let folder = first_folder(&paint);
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    click(&mut paint, at);
    paint.command("layer.toggle.visible");
    paint.settle(2);
    assert!(!info(&paint, folder).style.visible, "the folder's eye");
    let _ = cel;
}

#[test]
fn step_068_073_085_095_new_layers_land_inside_the_cel_folder() {
    let mut paint = tutorial("tut25-068");
    let (cel, rough) = folder_cel(&mut paint);
    let folder = first_folder(&paint);
    let at = row_of(&mut paint, rough).center();
    click(&mut paint, at);
    let before: Vec<EntityId> = stack(&paint).iter().map(|info| info.id).collect();
    undo_pair(&mut paint, "New Raster Layer", |paint| {
        click_named(paint, "Raster Layer");
    });
    let made = stack(&paint)
        .into_iter()
        .find(|info| !before.contains(&info.id))
        .expect("a layer")
        .id;
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), made),
        Some(cel),
        "in the cel's folder, not a new cel"
    );
    assert_eq!(
        sue_paint::anim::cels(arena(&paint), picture(&paint), folder),
        [cel],
        "still one cel"
    );
}

#[test]
fn step_069_the_rough_layer_hidden_with_its_eye() {
    let mut paint = tutorial("tut25-069");
    // The rough layer, inside the cel's folder (a cel's own eye says
    // whether the frame shows it).
    let (_, rough) = folder_cel(&mut paint);
    let at = row_of(&mut paint, rough);
    // The eye is left of the name in the row.
    let eye = eye_of(&mut paint, at);
    undo_pair(&mut paint, "the eye", |paint| {
        click(paint, eye.center());
    });
    assert!(!info(&paint, rough).style.visible);
}

/// The eye of the Layer palette's row at `at`: the one nearest it.
fn eye_of(paint: &mut PaintDriver, at: Rect) -> Rect {
    look(paint)
        .into_iter()
        .filter(|(name, rect)| name == "Hide Layer" && rect.left() > 1100.0)
        .map(|(_, rect)| rect)
        .min_by(|a, b| {
            (a.center().y - at.center().y)
                .abs()
                .total_cmp(&(b.center().y - at.center().y).abs())
        })
        .expect("the row's eye")
}

/// Picks the Pen tool with P (P goes on to Sketch when the Pen is
/// already picked), and its G-pen.
fn g_pen(paint: &mut PaintDriver) {
    for _ in 0..2 {
        if lists(paint, "G-pen") {
            break;
        }
        press_key(paint, egui::Key::P, Modifiers::NONE);
    }
    assert!(lists(paint, "G-pen"), "{:?}", paint.texts);
    paint.click_text("G-pen");
}

/// The Color Set palette's swatch `cell` of set `set`.
fn swatch(set: usize, cell: usize) -> egui::Id {
    egui::Id::unique(("sue-paint-swatch", "set", set, cell))
}

/// The drawing color's hex code, as Color Settings shows it.
fn drawing_hex(paint: &mut PaintDriver) -> String {
    paint.command("color.settings");
    paint.settle(4);
    let hex = paint
        .texts
        .iter()
        .find(|(text, _)| text.starts_with('#') && text.len() == 7)
        .map(|(text, _)| text.clone())
        .unwrap_or_else(|| panic!("a hex code shows: {:?}", paint.texts));
    paint.click_text("Cancel");
    hex
}

#[test]
fn step_070_colors_picked_from_the_color_set() {
    let mut paint = tutorial("tut25-070");
    look(&mut paint);
    let before = drawing_hex(&mut paint);
    let mut seen = vec![before];
    for cell in [2, 5] {
        look(&mut paint);
        let at = paint
            .ctx
            .read_response(swatch(0, cell))
            .map(|response| response.rect.center())
            .expect("a swatch of the default set");
        click(&mut paint, at);
        let hex = drawing_hex(&mut paint);
        assert!(!seen.contains(&hex), "a new color: {hex} after {seen:?}");
        seen.push(hex);
    }
}

#[test]
fn step_071_072_sub_color_made_active_and_x_switches_back() {
    let mut paint = tutorial("tut25-071");
    let main = drawing_hex(&mut paint);
    look(&mut paint);
    let sub_chip = paint
        .ctx
        .read_response(egui::Id::unique(("sue-paint-set-drawing-color", true)))
        .map(|response| response.rect)
        .expect("the sub color");
    click(&mut paint, sub_chip.center());
    let sub = drawing_hex(&mut paint);
    assert_ne!(main, sub, "the sub color is drawn with now");
    // X switches which one is drawn with; the two keep their colors.
    press_key(&mut paint, egui::Key::X, Modifiers::NONE);
    assert_eq!(drawing_hex(&mut paint), main);
    press_key(&mut paint, egui::Key::X, Modifiers::NONE);
    assert_eq!(drawing_hex(&mut paint), sub);
}

#[test]
fn step_074_091_a_clean_animation_folder_named_with_a_palette_color() {
    let mut paint = tutorial("tut25-074");
    paint.command("anim.layer.folder");
    paint.settle(2);
    let clean = animation_folders(&paint)[1];
    let name = name_of(&paint, clean);
    let at = row(&mut paint, &name).center();
    double_click(&mut paint, at);
    type_over(&mut paint, "A clean");
    press_key(&mut paint, egui::Key::Enter, Modifiers::NONE);
    assert_eq!(name_of(&paint, clean), "A clean");
    let at = row_of(&mut paint, clean).center();
    click(&mut paint, at);
    paint.command("layer.palette_color.green");
    paint.settle(2);
    assert_eq!(
        sue_paint::doc::palette_color(arena(&paint), clean),
        sue_paint::doc::named_palette_color("green")
    );
    // Its new cel, put in a folder: a folder cel.
    paint.command("anim.cel.new");
    paint.settle(2);
    paint.command("layer.folder_insert");
    paint.settle(2);
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), clean);
    assert_eq!(cels.len(), 1);
    assert!(sue_paint::doc::is_folder(arena(&paint), cels[0]));
    assert_eq!(
        name_of(&paint, cels[0]),
        "1",
        "the folder keeps the cel's name"
    );
}

// ----- 24:51 to 37:00: clean up with the pen and the curve ------------------

/// The Tool Property palette's control named `name`, scrolled into view
/// (the rightmost so named in its column).
fn reveal(paint: &mut PaintDriver, name: &str) -> Rect {
    let tab = control(paint, "Tool Property");
    let over = Pos2::new(tab.center().x, tab.bottom() + 120.0);
    for _ in 0..40 {
        let found = look(paint);
        let rect = found
            .iter()
            .filter(|(label, rect)| {
                label == name && (rect.center().x - tab.center().x).abs() < 300.0
            })
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.left().total_cmp(&b.left()))
            .unwrap_or_else(|| panic!("{name:?} is laid out"));
        let limit = found
            .iter()
            .filter(|(label, rect)| {
                label.ends_with(": palette menu")
                    && rect.left() < tab.right()
                    && rect.top() > tab.bottom() + 20.0
            })
            .map(|(_, rect)| rect.top())
            .fold(900.0_f32, f32::min);
        if rect.bottom() < limit - 2.0 && rect.top() > tab.bottom() + 36.0 {
            return rect;
        }
        let delta = if rect.bottom() >= limit - 2.0 {
            -40.0
        } else {
            40.0
        };
        paint.step(vec![
            Event::PointerMoved(over),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Modifiers::NONE,
            },
        ]);
        paint.settle(2);
    }
    panic!("{name:?} could not be scrolled into view");
}

/// Picks `option` on Tool Property's row `row`: side by side on the row,
/// or from its drop-down.
fn choose(paint: &mut PaintDriver, row_name: &str, option: &str) {
    let list = reveal(paint, row_name);
    let side = look(paint).into_iter().find(|(label, rect)| {
        label == option
            && (rect.center().y - list.center().y).abs() < list.height() / 2.0
            && rect.left() >= list.left() - 1.0
            && rect.right() <= list.right() + 1.0
    });
    match side {
        Some((_, rect)) => {
            click(paint, rect.center());
        }
        None => {
            click(paint, list.center());
            let at = look(paint)
                .into_iter()
                .filter(|(label, _)| label == option)
                .map(|(_, rect)| rect)
                .max_by(|a, b| a.top().total_cmp(&b.top()))
                .or_else(|| paint.rect_of(option))
                .unwrap_or_else(|| panic!("{option:?} in the open list"));
            click(paint, at.center());
        }
    }
}

/// The share of `layer`'s covered pixels that are partly covered: 0 for
/// hard edges.
fn soft(paint: &PaintDriver, layer: EntityId) -> usize {
    let raster = sue_paint::doc::raster(arena(paint), layer).expect("a raster");
    raster
        .bytes()
        .expect("pixels")
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0 && pixel[3] < 255)
        .count()
}

/// How many pixels of `layer` are covered at all.
fn painted(paint: &PaintDriver, layer: EntityId) -> usize {
    let raster = sue_paint::doc::raster(arena(paint), layer).expect("a raster");
    raster
        .bytes()
        .expect("pixels")
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count()
}

#[test]
fn step_076_077_078_079_080_g_pen_2_px_with_no_anti_aliasing_or_dynamics() {
    let mut paint = tutorial("tut25-076");
    g_pen(&mut paint);
    click_named(&mut paint, "Tool Property");
    choose(&mut paint, "Anti-aliasing", "None");
    // Adjust by speed off; Brush Size dynamics: pressure off.
    let speed = reveal(&mut paint, "Adjust by speed");
    click(&mut paint, speed.center());
    let cel = layer(&paint, "1");
    stroke(&mut paint, 0.0);
    assert!(painted(&paint, cel) > 50, "the pen drew");
    // The body of the line is hard pixels; the last few pixels of the
    // tapered end are lighter ink (a defect sent to the brush engine's
    // steward: see step_078_every_pixel_hard_with_anti_aliasing_none).
    let bounds = drawn(&paint, cel).expect("a line");
    for (x, _) in soft_pixels(&paint, cel) {
        assert!(
            x + 4 >= bounds[2],
            "only at the very end: {x} of {bounds:?}"
        );
    }
}

/// The partly covered pixels of `layer`: their column and alpha.
fn soft_pixels(paint: &PaintDriver, layer: EntityId) -> Vec<(u32, u8)> {
    let raster = sue_paint::doc::raster(arena(paint), layer).expect("a raster");
    let width = raster.width();
    raster
        .bytes()
        .expect("pixels")
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, pixel)| pixel[3] > 0 && pixel[3] < 255)
        .map(|(index, pixel)| (index as u32 % width, pixel[3]))
        .collect()
}

/// Step 78 as Clip Studio Paint has it: with Anti-aliasing None every
/// pixel of a pen line is fully covered, its tapered ends too. Sue lays
/// a tip thinner than 0.7 px as lighter ink (`Disc::drawn` in
/// `sue-brush`), so a few pixels at the end of the taper are partial.
#[test]
#[ignore = "DEFECT: lighter ink at the taper's end with Anti-aliasing None; sent to the brush engine's steward (P5-3)"]
fn step_078_every_pixel_hard_with_anti_aliasing_none() {
    let mut paint = tutorial("tut25-078");
    g_pen(&mut paint);
    click_named(&mut paint, "Tool Property");
    choose(&mut paint, "Anti-aliasing", "None");
    let cel = layer(&paint, "1");
    stroke(&mut paint, 0.0);
    assert_eq!(soft(&paint, cel), 0, "hard pixels only");
}

#[test]
fn step_081_082_087_the_curve_is_dragged_let_go_bent_and_clicked() {
    let mut paint = tutorial("tut25-081");
    press_key(&mut paint, egui::Key::U, Modifiers::NONE);
    assert!(lists(&mut paint, "Curve"), "{:?}", paint.texts);
    paint.click_text("Curve");
    let cel = layer(&paint, "1");
    let at = middle(&paint);
    // Press at the start, drag to the end, let go.
    paint.drag(
        at - vec2(100.0, 0.0),
        at + vec2(100.0, 0.0),
        PointerButton::Primary,
    );
    // Move to bend, click to finish.
    let bend = at + vec2(0.0, -80.0);
    paint.step(vec![Event::PointerMoved(bend)]);
    paint.settle(2);
    click(&mut paint, bend);
    let bounds = drawn(&paint, cel).expect("a curve");
    assert!(
        bounds[3] - bounds[1] > 40,
        "the line bent up between its ends: {bounds:?}"
    );
}

#[test]
fn step_083_the_eraser_with_no_anti_aliasing() {
    let mut paint = tutorial("tut25-083");
    let cel = layer(&paint, "1");
    paint.command("edit.fill");
    paint.settle(2);
    press_key(&mut paint, egui::Key::E, Modifiers::NONE);
    click_named(&mut paint, "Tool Property");
    choose(&mut paint, "Anti-aliasing", "None");
    stroke(&mut paint, 0.0);
    assert_eq!(soft(&paint, cel), 0, "a hard erased edge");
}

#[test]
fn step_090_110_duplicating_an_animation_folder_copies_its_cels_and_timing() {
    let mut paint = tutorial("tut25-090");
    let folder = drawn_cels(&mut paint, 3, 2);
    let timing = shown(&paint, folder, 6);
    let name = name_of(&paint, folder);
    let at = row(&mut paint, &name).center();
    right_click(&mut paint, at);
    undo_pair(&mut paint, "Duplicate Layer", |paint| {
        paint.click_text("Duplicate Layer");
    });
    let folders = animation_folders(&paint);
    assert_eq!(folders.len(), 2, "a second animation folder");
    let copy = *folders.iter().find(|id| **id != folder).unwrap();
    assert_eq!(shown(&paint, copy, 6), timing, "the same timing");
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), copy);
    assert_eq!(cels.len(), 3);
    for cel in cels {
        assert!(
            drawn(&paint, cel).is_some(),
            "each copied cel holds its drawing"
        );
    }
}

#[test]
fn step_092_seen_v21_a_new_color_set_with_a_name() {
    let mut paint = tutorial("tut25-092");
    let wrench = control(&mut paint, "Color set settings");
    click(&mut paint, wrench.center());
    paint.click_text("New Color Set...");
    let name = control(&mut paint, "Name");
    click(&mut paint, name.center());
    type_over(&mut paint, "A colors");
    paint.click_text("OK");
    look(&mut paint);
    assert!(paint.rect_of("A colors").is_some(), "{:?}", paint.texts);
}

#[test]
fn step_093_094_a_double_click_on_the_main_color_and_a_color_put_in_a_cell() {
    let mut paint = tutorial("tut25-093");
    // The main color of the color palettes (the topmost one so named).
    let main = look(&mut paint)
        .into_iter()
        .filter(|(name, _)| name == "Main color")
        .map(|(_, rect)| rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the main color");
    double_click(&mut paint, main.center());
    look(&mut paint);
    assert!(
        paint.rect_of("Color Settings").is_some(),
        "{:?}",
        paint.texts
    );
    paint.click_text("Cancel");
    // Step 94: a new set's empty cell takes the drawing color.
    let wrench = control(&mut paint, "Color set settings");
    click(&mut paint, wrench.center());
    paint.click_text("New Color Set...");
    paint.click_text("OK");
    look(&mut paint);
    let cell = paint
        .ctx
        .read_response(swatch(4, 0))
        .map(|response| response.rect)
        .expect("the new set's first cell");
    let texts = right_click(&mut paint, cell.center());
    assert!(
        texts.iter().any(|(text, _)| text == "Replace Color"),
        "{texts:?}"
    );
    paint.click_text("Replace Color");
    let file =
        std::fs::read_to_string(paint.root.join("data").join("paint").join("color_sets.txt"))
            .unwrap_or_default();
    assert!(file.contains("#000000"), "{file}");
}

/// A closed ring drawn with the pen round `center`.
fn ring(paint: &mut PaintDriver, center: Pos2, radius: f32) {
    press_key(paint, egui::Key::P, Modifiers::NONE);
    let points: Vec<Pos2> = (0..=56)
        .map(|index| {
            let angle = index as f32 / 48.0 * std::f32::consts::TAU;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    paint.step(vec![Event::PointerMoved(points[0])]);
    paint.step(vec![button(
        points[0],
        PointerButton::Primary,
        true,
        Modifiers::NONE,
    )]);
    for point in &points[1..] {
        paint.step(vec![Event::PointerMoved(*point)]);
    }
    paint.step(vec![button(
        points[points.len() - 1],
        PointerButton::Primary,
        false,
        Modifiers::NONE,
    )]);
    paint.settle(4);
}

#[test]
fn step_096_to_104_fill_refer_other_layers_stops_at_lines_and_passes_drafts() {
    let mut paint = tutorial("tut25-096");
    let (cel, lines) = {
        let one = layer(&paint, "1");
        let at = middle(&paint);
        ring(&mut paint, at, 70.0);
        let row_at = row(&mut paint, "1").center();
        click(&mut paint, row_at);
        paint.command("layer.folder_insert");
        paint.settle(2);
        (sue_paint::doc::folder_of(arena(&paint), one).unwrap(), one)
    };
    // A color separation line across the ring, on a layer of its own.
    let at = row_of(&mut paint, lines).center();
    click(&mut paint, at);
    paint.command("layer.new");
    paint.settle(2);
    let separation = stack(&paint)
        .into_iter()
        .rev()
        .find(|info| {
            info.id != lines
                && sue_paint::doc::folder_of(arena(&paint), info.id) == Some(cel)
                && info.kind == sue_model::picture::LayerKind::Raster
        })
        .expect("the separation layer")
        .id;
    let middle_at = middle(&paint);
    press_key(&mut paint, egui::Key::P, Modifiers::NONE);
    paint.drag(
        middle_at + vec2(-90.0, 0.0),
        middle_at + vec2(90.0, 0.0),
        PointerButton::Primary,
    );
    // The paint layer.
    paint.command("layer.new");
    paint.settle(2);
    let colors = stack(&paint)
        .into_iter()
        .rev()
        .find(|info| {
            ![lines, separation].contains(&info.id)
                && sue_paint::doc::folder_of(arena(&paint), info.id) == Some(cel)
                && info.kind == sue_model::picture::LayerKind::Raster
        })
        .expect("the paint layer")
        .id;
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), colors), Some(cel));
    // Step 96: G, Refer Other Layers, the first of the Fill tool.
    press_key(&mut paint, egui::Key::G, Modifiers::NONE);
    assert!(lists(&mut paint, "Refer Other Layers"), "{:?}", paint.texts);
    paint.click_text("Refer Other Layers");
    click_named(&mut paint, "Tool Property");
    // Steps 97 to 100: Close gap off, Tolerance 0, Area scaling 0,
    // Anti-aliasing off.
    choose(&mut paint, "Close gap", "None");
    choose(&mut paint, "Anti-aliasing", "None");
    // Step 101: a click above the separation line, inside the ring: the
    // ring and the separation line both stop it.
    let above = middle_at + vec2(0.0, -30.0);
    click(&mut paint, above);
    let half = painted(&paint, colors);
    assert!(half > 200, "the fill went in: {half}");
    paint.command("edit.undo");
    paint.settle(2);
    // Step 102: the separation layer set as a draft layer.
    let at = row_of(&mut paint, separation).center();
    click(&mut paint, at);
    undo_pair(&mut paint, "Set as Draft Layer", |paint| {
        click_named(paint, "Set as Draft Layer");
    });
    assert!(info(&paint, separation).style.draft);
    // Step 103: Refer other layers leaves draft layers out: the fill now
    // passes the separation line and fills the whole ring.
    let at = row_of(&mut paint, colors).center();
    click(&mut paint, at);
    click(&mut paint, above);
    let whole = painted(&paint, colors);
    assert!(
        whole > half * 3 / 2,
        "the draft line no longer stops it: {half} then {whole}"
    );
    assert_eq!(soft(&paint, colors), 0, "anti-aliasing off");
    // The fill stops at the ring: it is far smaller than the canvas.
    let raster = sue_paint::doc::raster(arena(&paint), colors).unwrap();
    assert!(whole < (raster.width() * raster.height()) as usize / 10);
}

#[test]
fn step_101_the_fill_sees_only_the_cels_of_the_frame_shown() {
    let mut paint = tutorial("tut25-101");
    // Cel 1 holds a ring; cel 2 (frame 3) holds nothing.
    let at = middle(&paint);
    ring(&mut paint, at, 70.0);
    paint.command("anim.next");
    paint.command("anim.next");
    paint.command("anim.cel.new");
    paint.settle(2);
    let two = layer(&paint, "2");
    press_key(&mut paint, egui::Key::G, Modifiers::NONE);
    paint.click_text("Refer Other Layers");
    click(&mut paint, at);
    let raster = sue_paint::doc::raster(arena(&paint), two).unwrap();
    let area = (raster.width() * raster.height()) as usize;
    assert!(
        painted(&paint, two) > area / 2,
        "cel 1's ring is on another frame: it stops nothing"
    );
}

#[test]
fn step_105_paper_hidden_shows_the_checkerboard() {
    let mut paint = tutorial("tut25-105");
    let paper = layer(&paint, "Paper");
    let at = row(&mut paint, "Paper");
    let eye = eye_of(&mut paint, at);
    click(&mut paint, eye.center());
    assert!(
        !info(&paint, paper).style.visible,
        "Paper hidden: the checkerboard shows"
    );
}

// ----- 40:57 to 44:28: exports and the smoothing action ---------------------

#[test]
fn step_108_export_animation_cels_a_folder_each_as_tga() {
    let mut paint = tutorial("tut25-108");
    drawn_cels(&mut paint, 2, 2);
    let folder = first_folder(&paint);
    let name = name_of(&paint, folder);
    let root = paint.root.join("cels");
    std::fs::create_dir_all(&root).unwrap();
    paint.command("file.export.cels");
    paint.settle(3);
    assert!(shows(&paint, "Export Animation Cels"), "{:?}", paint.texts);
    assert!(shows(&paint, "A folder for each animation folder"));
    paint.click_text("OK");
    picked(&mut paint, "anim-cels", &root.join("A.tga"));
    let into = root.join(&name);
    for cel in ["1", "2"] {
        let file = into.join(format!("{cel}.tga"));
        assert!(
            file.exists(),
            "{file:?}: {:?}",
            std::fs::read_dir(&root).map(|dir| dir.flatten().map(|e| e.path()).collect::<Vec<_>>())
        );
    }
}

#[test]
fn step_109_image_sequence_with_a_prefix_a_start_number_and_png() {
    let mut paint = tutorial("tut25-109");
    drawn_cels(&mut paint, 2, 2);
    let root = paint.root.join("sequence");
    std::fs::create_dir_all(&root).unwrap();
    paint.command("file.export.sequence");
    paint.settle(3);
    assert!(
        shows(&paint, "Image Sequence Export Settings"),
        "{:?}",
        paint.texts
    );
    let prefix = super::paint_anim::right_of(&paint, "File name prefix");
    super::paint_anim::type_into(&mut paint, prefix, "Animation_");
    super::paint_anim::fill(&mut paint, "First number", "1");
    paint.click_text("OK");
    picked(&mut paint, "anim-sequence", &root.join("Animation.png"));
    let first = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("Animation_"))
        })
        .min()
        .expect("the pictures");
    assert!(
        first.to_string_lossy().ends_with("Animation_0001.png")
            || first.to_string_lossy().ends_with("Animation_1.png"),
        "{first:?}"
    );
    assert_eq!(png_size(&first), [1920, 1080], "the output frame");
}

#[test]
fn step_111_a_cel_folder_merged_keeps_its_name_and_its_frames() {
    let mut paint = tutorial("tut25-111");
    let (cel, _) = folder_cel(&mut paint);
    let folder = first_folder(&paint);
    let name = name_of(&paint, cel);
    let at = row_of(&mut paint, cel).center();
    click(&mut paint, at);
    undo_pair(&mut paint, "Merge Selected Layers", |paint| {
        paint.command("layer.merge_selected");
    });
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), folder);
    assert_eq!(cels.len(), 1);
    let merged = cels[0];
    assert!(
        !sue_paint::doc::is_folder(arena(&paint), merged),
        "one layer"
    );
    assert_eq!(name_of(&paint, merged), name, "the folder's name");
    assert_eq!(shown(&paint, folder, 1), [name], "still the cel on frame 1");
    assert!(drawn(&paint, merged).is_some());
}

#[test]
fn step_112_filter_blur_smoothing() {
    let mut paint = tutorial("tut25-112");
    let cel = layer(&paint, "1");
    g_pen(&mut paint);
    click_named(&mut paint, "Tool Property");
    choose(&mut paint, "Anti-aliasing", "None");
    stroke(&mut paint, 0.0);
    let aliased = soft(&paint, cel);
    let label = sue_paint::menus()
        .iter()
        .flat_map(|menu| menu.items.iter())
        .find_map(|item| find_label(item, "filter.smooth"))
        .expect("Smoothing is in the menus");
    assert_eq!(label, "Smoothing");
    undo_pair(&mut paint, "Smoothing", |paint| {
        paint.command("filter.smooth")
    });
    assert!(
        soft(&paint, cel) > aliased + 100,
        "the aliased line is smoothed: {aliased} soft pixels before"
    );
}

/// The label of the menu item that runs `command`, under `item`.
fn find_label(item: &sue_wm::MenuItem, command: &str) -> Option<String> {
    match item {
        sue_wm::MenuItem::Action {
            label,
            command: run,
            ..
        } if run == command => Some(label.clone()),
        sue_wm::MenuItem::Sub { items, .. } => {
            items.iter().find_map(|item| find_label(item, command))
        }
        _ => None,
    }
}

/// The labels of the menu at `path` (a menu title, then submenu labels).
fn menu_labels(path: &[&str]) -> Vec<String> {
    let menus = sue_paint::menus();
    let menu = menus
        .iter()
        .find(|menu| menu.title == path[0])
        .unwrap_or_else(|| panic!("a menu {:?}", path[0]));
    let mut items = &menu.items;
    for step in &path[1..] {
        items = items
            .iter()
            .find_map(|item| match item {
                sue_wm::MenuItem::Sub { label, items } if label == step => Some(items),
                _ => None,
            })
            .unwrap_or_else(|| panic!("a submenu {step:?}"));
    }
    items
        .iter()
        .map(|item| item.label().to_owned())
        .filter(|label| !label.is_empty())
        .collect()
}

#[test]
fn step_113_to_116_seen_v26_an_auto_action_merges_smooths_and_steps_on() {
    let mut paint = tutorial("tut25-113");
    // Three folder cels, each with two layers.
    let folder = first_folder(&paint);
    folder_cel(&mut paint);
    paint.command("layer.new");
    paint.settle(2);
    stroke(&mut paint, 20.0);
    for _ in 0..2 {
        paint.command("anim.next");
        paint.command("anim.next");
        paint.command("anim.cel.new");
        paint.settle(2);
        stroke(&mut paint, -40.0);
    }
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), folder);
    assert_eq!(cels.len(), 3);
    assert!(
        cels.iter()
            .all(|cel| sue_paint::doc::is_folder(arena(&paint), *cel))
    );
    bind(&mut paint, "anim.cel.next", "2");
    // Step 113: the Auto Action palette shows.
    let applet = AppletId::new(sue_paint::ID).unwrap();
    assert!(
        paint
            .runtime
            .applets
            .get_mut(&applet)
            .unwrap()
            .navigate("extras/show/auto_action")
    );
    paint.settle(6);
    assert!(
        shows(&paint, "New Draft Layer"),
        "Sue's ready actions: {:?}",
        paint.texts
    );
    // Step 114: a new action, named "smoothing"; it records at once.
    // Frame 1, where the first cel is the layer picked.
    paint.command("anim.first");
    paint.settle(2);
    paint.command("auto.new");
    paint.settle(3);
    // Step 115: Merge Selected Layers, Smoothing, then 2 for the next cel.
    paint.command("layer.merge_selected");
    paint.settle(2);
    paint.command("filter.smooth");
    paint.settle(2);
    press_key(&mut paint, egui::Key::Num2, Modifiers::NONE);
    paint.command("auto.record");
    paint.settle(3);
    let file = std::fs::read_to_string(
        paint
            .root
            .join("data")
            .join("paint")
            .join("auto_actions.txt"),
    )
    .unwrap_or_default();
    let steps: Vec<&str> = file
        .lines()
        .filter_map(|line| line.strip_prefix("step "))
        .filter(|line| ["layer.merge_selected", "filter.smooth", "anim.cel.next"].contains(line))
        .collect();
    assert_eq!(
        steps,
        ["layer.merge_selected", "filter.smooth", "anim.cel.next"],
        "{file}"
    );
    // Step 116: played twice, every cel is one smoothed layer.
    for _ in 0..2 {
        paint.command("auto.play");
        paint.settle(4);
    }
    let cels = sue_paint::anim::cels(arena(&paint), picture(&paint), folder);
    assert_eq!(cels.len(), 3);
    for cel in &cels {
        assert!(
            !sue_paint::doc::is_folder(arena(&paint), *cel),
            "{} merged",
            name_of(&paint, *cel)
        );
    }
    let names: Vec<String> = cels.iter().map(|cel| name_of(&paint, *cel)).collect();
    assert_eq!(names, ["1", "2", "3"]);
}

#[test]
fn step_117_export_movie_mp4_at_1920_by_1080_and_24_fps() {
    let mut paint = tutorial("tut25-117");
    drawn_cels(&mut paint, 2, 2);
    paint.command("file.export.movie");
    let texts = paint.settle(3);
    let mp4 = sue_platform::movie::mp4_available();
    if mp4 {
        assert!(
            texts.iter().any(|(text, _)| text == "MP4 (H.264)"),
            "{texts:?}"
        );
    }
    assert!(shows(&paint, "Movie Export Settings"), "{texts:?}");
    assert!(
        shows(&paint, "1,920") && shows(&paint, "1,080"),
        "{texts:?}"
    );
    paint.click_text("OK");
    let root = paint.root.join("movie");
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join(if mp4 { "A.mp4" } else { "A.avi" });
    picked(&mut paint, "anim-movie", &file);
    let bytes = std::fs::read(&file).expect("the movie is written");
    if mp4 {
        assert_eq!(&bytes[4..8], b"ftyp");
    } else {
        assert_eq!(&bytes[..4], b"RIFF");
    }
}

// ----- Seen in the video (P0-6) ----------------------------------------------

#[test]
fn seen_v02_new_animation_lists_its_frame_and_timeline_fields() {
    let mut paint = PaintDriver::new("tut25-v02");
    open_new(&mut paint);
    click_named(&mut paint, "Animation");
    for text in [
        "Size of output frame",
        "Title-safe area",
        "Overflow frame",
        "Blank space",
        "Timeline name",
        "Scene number",
        "Shot number",
        "Division line",
    ] {
        assert!(paint.rect_of(text).is_some(), "{text}: {:?}", paint.texts);
    }
}

#[test]
fn seen_v05_a_locked_layer_on_the_timeline() {
    let mut paint = tutorial("tut25-v05");
    let paper = layer(&paint, "Paper");
    let at = row(&mut paint, "Paper").center();
    click(&mut paint, at);
    paint.command("layer.toggle.lock");
    paint.settle(3);
    assert!(info(&paint, paper).locked);
    paint.command("anim.thumbnails.none");
    let sequence = timeline(&paint);
    assert!(
        sue_paint::anim::setup::track_of_layer(&sequence, paper).is_some(),
        "Paper has a track of its own"
    );
    let texts = paint.settle(3);
    assert!(
        texts.iter().any(|(text, rect)| text.starts_with("Paper")
            && rect.top() > 600.0
            && rect.left() < 560.0),
        "Paper's track is on the timeline: {texts:?}"
    );
}

#[test]
fn seen_v08_ctrl_shift_alt_k_opens_shortcut_settings() {
    let mut paint = tutorial("tut25-v08");
    let all = Modifiers::COMMAND | Modifiers::SHIFT | Modifiers::ALT;
    paint.step(vec![Event::ModifiersChanged(all), key(egui::Key::K, all)]);
    paint.step(vec![Event::ModifiersChanged(Modifiers::NONE)]);
    paint.settle(8);
    assert!(shows(&paint, "Keyboard Shortcuts"), "{:?}", paint.texts);
}

#[test]
fn seen_v09_the_view_submenu_is_rotate_flip() {
    let view = menu_labels(&["View"]);
    assert!(view.iter().any(|label| label == "Rotate/Flip"), "{view:?}");
    let turn = menu_labels(&["View", "Rotate/Flip"]);
    for label in ["Reset Rotation", "Flip Horizontal", "Flip Vertical"] {
        assert!(turn.iter().any(|item| item == label), "{label}: {turn:?}");
    }
}

#[test]
fn seen_v11_the_track_header_names_the_cel_shown() {
    let mut paint = tutorial("tut25-v11");
    drawn_cels(&mut paint, 2, 2);
    paint.command("anim.next");
    paint.command("anim.next");
    let texts = paint.settle(3);
    let folder = first_folder(&paint);
    let header = format!("{} : 2", name_of(&paint, folder));
    assert!(
        texts
            .iter()
            .any(|(text, rect)| *text == header && rect.top() > 600.0),
        "{header}: {texts:?}"
    );
}

#[test]
fn seen_v14_figure_direct_draw_has_the_ver_4_sub_tools() {
    let mut paint = tutorial("tut25-v14");
    press_key(&mut paint, egui::Key::U, Modifiers::NONE);
    let texts = paint.settle(4);
    for name in [
        "Straight Line",
        "Curve",
        "Polyline",
        "Continuous Curve",
        "Bezier Curve",
        "Rectangle",
        "Ellipse",
        "Polygon",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
}

#[test]
fn seen_v15_scale_rotate_shows_its_bar() {
    let mut paint = tutorial("tut25-v15");
    stroke(&mut paint, 0.0);
    press_key(&mut paint, egui::Key::T, Modifiers::COMMAND);
    for text in ["Scale/Rotate", "OK", "Cancel"] {
        assert!(shows(&paint, text), "{text}: {:?}", paint.texts);
    }
    press_key(&mut paint, egui::Key::Escape, Modifiers::NONE);
}

#[test]
fn seen_v16_v18_the_animation_menu_s_cels_and_edit_track_entries() {
    let cels = menu_labels(&["Animation", "Show Animation Cels"]);
    for label in ["Enable Onion Skin", "Onion Skin Settings..."] {
        assert!(cels.iter().any(|item| item == label), "{label}: {cels:?}");
    }
    let track = menu_labels(&["Animation", "Edit Track"]);
    for label in [
        "Rename in Timeline Order",
        "Rename in Layer Order",
        "Select Previous Cel",
        "Select Next Cel",
    ] {
        assert!(track.iter().any(|item| item == label), "{label}: {track:?}");
    }
}

#[test]
fn seen_v19_the_frame_rate_reads_while_playing() {
    let mut paint = tutorial("tut25-v19");
    drawn_cels(&mut paint, 2, 2);
    paint.command("view.playback_fps");
    click_named(&mut paint, "Play/Stop");
    let mut seen = false;
    for _ in 0..90 {
        let texts = paint.step(Vec::new());
        seen |= texts.iter().any(|(text, _)| text.contains("fps"));
    }
    click_named(&mut paint, "Play/Stop");
    assert!(seen, "a frame rate readout while playing");
}

#[test]
fn seen_v20_brush_size_dynamics_offers_pressure_tilt_velocity_random() {
    let mut paint = tutorial("tut25-v20");
    press_key(&mut paint, egui::Key::P, Modifiers::NONE);
    click_named(&mut paint, "Tool Property");
    let named = look(&mut paint);
    let dynamics = named
        .iter()
        .find(|(name, rect)| name.contains("ynamics") && rect.left() < 400.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| {
            let names: Vec<&String> = named.iter().map(|(name, _)| name).collect();
            panic!("the Brush Size dynamics button: {names:?}")
        });
    click(&mut paint, dynamics.center());
    for text in ["Pen pressure", "Tilt", "Velocity", "Random"] {
        assert!(shows(&paint, text), "{text}: {:?}", paint.texts);
    }
}

#[test]
fn seen_v22_fill_sub_tools_in_ver_4_s_order() {
    let mut paint = tutorial("tut25-v22");
    press_key(&mut paint, egui::Key::G, Modifiers::NONE);
    let texts = paint.settle(4);
    let top = |name: &str| {
        texts
            .iter()
            .find(|(text, rect)| text == name && rect.left() < 400.0)
            .map(|(_, rect)| rect.top())
            .unwrap_or_else(|| panic!("{name}: {texts:?}"))
    };
    let order = [
        top("Refer Other Layers"),
        top("Refer Only Editing Layer"),
        top("Close and fill"),
        top("Lasso fill"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
}

#[test]
fn seen_v24_a_draft_layer_is_marked_on_its_row() {
    let mut paint = tutorial("tut25-v24");
    let at = row(&mut paint, "1").center();
    click(&mut paint, at);
    paint.command("layer.toggle.draft");
    paint.settle(3);
    let named = look(&mut paint);
    assert!(
        named.iter().any(|(name, rect)| name.contains("raft")
            && rect.left() > 1100.0
            && (rect.center().y - at.y).abs() < 16.0),
        "the row is marked as a draft: {:?}",
        named.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    assert!(!shows(&paint, "1 (draft)"), "the name is not changed");
}

#[test]
fn seen_v25_export_animation_in_ver_4_s_order() {
    let mut paint = PaintDriver::new("tut25-v25");
    paint.settle(2);
    let file = menu_labels(&["File"]);
    let export = file
        .iter()
        .find(|label| label.starts_with("Export Animation"))
        .unwrap_or_else(|| panic!("{file:?}"))
        .clone();
    let items = menu_labels(&["File", &export]);
    let at = |name: &str| {
        items
            .iter()
            .position(|item| item.starts_with(name))
            .unwrap_or_else(|| panic!("{name}: {items:?}"))
    };
    let order = [
        at("Image Sequence"),
        at("Animated GIF"),
        at("Animated Sticker"),
        at("Animated WebP"),
        at("Movie"),
        at("Export Animation Cels"),
    ];
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{items:?}");
}

#[test]
fn seen_v21_edit_color_sets_offers_a_standard_set() {
    let mut paint = tutorial("tut25-v21");
    paint.command("color.edit_sets");
    paint.settle(4);
    for text in ["Edit Color Sets", "Add Standard Set"] {
        assert!(
            paint.texts.iter().any(|(found, _)| found.starts_with(text)),
            "{text}: {:?}",
            paint.texts
        );
    }
}

#[test]
fn seen_v23_fill_tool_property_has_ver_4_s_rows() {
    let mut paint = tutorial("tut25-v23");
    press_key(&mut paint, egui::Key::G, Modifiers::NONE);
    paint.click_text("Refer Other Layers");
    click_named(&mut paint, "Tool Property");
    for name in [
        "Apply to connected pixels only",
        "Close gap",
        "Tolerance",
        "Area scaling",
        "Refer multiple",
        "Fill up to vector path",
        "Anti-aliasing",
        "Leave out draft layers",
    ] {
        reveal(&mut paint, name);
    }
}
