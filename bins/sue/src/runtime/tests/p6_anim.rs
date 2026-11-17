//! Phase 6, "Timeline leftovers": the rows of the timeline, animator
//! setup and rigs tutorials (and the two animation tutorials) that were
//! still open after Phase 5, each closed by a test named after its gap id
//! that drives real frames of the running Sue Paint. Nothing of the
//! videos is quoted here (rule 10).

use super::paint_anim::{arena_bytes, click_at, line_on_canvas, open_picture};
use super::*;

// What the tests read of the document.

fn arena(paint: &PaintDriver) -> &sue_core::Arena {
    &paint.runtime.engine.world.arena
}

/// The picture's timeline, cloned.
fn timeline(paint: &PaintDriver) -> sue_model::Sequence {
    sue_paint::anim::timeline(arena(paint), open_picture(paint))
        .expect("a timeline")
        .clone()
}

/// The layer, folder or cel named `name`.
fn named(paint: &PaintDriver, name: &str) -> sue_core::EntityId {
    let picture = open_picture(paint);
    sue_paint::doc::layers(arena(paint), picture)
        .into_iter()
        .find(|info| info.name == name)
        .map(|info| info.id)
        .unwrap_or_else(|| panic!("a layer named {name:?}"))
}

/// The first animation folder's track.
fn folder_track(paint: &PaintDriver) -> usize {
    sue_paint::anim::folders(&timeline(paint))[0]
}

/// The cel the first animation folder shows at `frame` (0 based).
fn shown_at(paint: &PaintDriver, frame: u32) -> Option<sue_core::EntityId> {
    let sequence = timeline(paint);
    sue_paint::anim::cel_at(&sequence.tracks[folder_track(paint)], sequence.rate, frame)
}

/// Runs `work` as one undo step: undo brings back the arena as it was,
/// ids included, and redo the arena as `work` left it.
fn undo_pair(paint: &mut PaintDriver, what: &str, work: impl FnOnce(&mut PaintDriver)) {
    let (before, steps) = (arena_bytes(paint), paint.runtime.engine.history.depth());
    work(paint);
    paint.settle(4);
    let after = arena_bytes(paint);
    assert!(after != before, "{what} changes the document");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        steps + 1,
        "{what} is one undo step"
    );
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(paint) == before, "{what} undone");
    paint.command("edit.redo");
    paint.settle(2);
    assert!(arena_bytes(paint) == after, "{what} redone");
}

/// An animation folder with `count` cels, cel n keyed at frame
/// `1 + (n - 1) * every`, each with a line drawn on it at its own height.
fn folder_with_cels(paint: &mut PaintDriver, count: u32, every: u32) {
    paint.command("anim.layer.folder");
    paint.settle(2);
    for index in 0..count {
        if index > 0 {
            for _ in 0..every {
                paint.command("anim.next");
            }
        }
        paint.command("anim.cel.new");
        paint.settle(2);
        line_on_canvas(paint, -60.0 + 30.0 * index as f32);
    }
    paint.command("anim.first");
    paint.settle(2);
}

/// Clicks the field at `rect`, types `text` over what it holds and ends
/// typing with Enter.
fn type_enter(paint: &mut PaintDriver, rect: egui::Rect, text: &str) {
    let at = rect.center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![button(true)]);
    paint.step(vec![button(false)]);
    paint.settle(2);
    paint.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    paint.step(vec![egui::Event::Text(text.to_owned())]);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(3);
}

/// Where a mark the frame drew was (`mark.<what>`), if it was.
fn mark(paint: &PaintDriver, id: &str) -> Option<Pos2> {
    sue_paint::shown_buttons(&paint.ctx)
        .into_iter()
        .find(|(known, _)| known == id)
        .map(|(_, rect)| rect.center())
}

/// Tool Property's label `label` (the left column of the page),
/// scrolled into view.
fn in_view(paint: &mut PaintDriver, label: &str) -> egui::Rect {
    let (top, bottom) = (120.0, 560.0);
    for _ in 0..40 {
        let found = paint
            .settle(2)
            .into_iter()
            .filter(|(text, rect)| text == label && rect.left() < 300.0)
            .map(|(_, rect)| rect)
            .next();
        let by = match found {
            Some(rect) if rect.top() >= top && rect.bottom() + 30.0 <= bottom => {
                // Let the scrolling come to rest before anything is
                // clicked there.
                let rect = paint
                    .settle(20)
                    .into_iter()
                    .filter(|(text, rect)| text == label && rect.left() < 300.0)
                    .map(|(_, rect)| rect)
                    .next()
                    .unwrap_or(rect);
                return rect;
            }
            Some(rect) if rect.top() < top => 40.0,
            _ => -40.0,
        };
        paint.step(vec![
            egui::Event::PointerMoved(Pos2::new(100.0, (top + bottom) / 2.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, by),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
    }
    panic!("{label:?} never scrolled into view: {:?}", paint.texts);
}

/// The number field that belongs to Tool Property's label `label`: the
/// number drawn under it (or beside it, in a wide palette).
fn field_under(paint: &mut PaintDriver, label: &str) -> egui::Rect {
    let at = in_view(paint, label);
    paint
        .texts
        .iter()
        .filter(|(text, rect)| {
            text.trim().replace(',', "").parse::<f64>().is_ok()
                && rect.left() < 300.0
                && rect.center().y > at.top()
                && rect.center().y < at.bottom() + 34.0
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("the field of {label:?}: {:?}", paint.texts))
}

/// A camera folder over an animation folder of one cel, picked, with the
/// Object tool and Tool Property up.
fn camera_picked(paint: &mut PaintDriver) -> sue_core::EntityId {
    paint.new_timeline();
    folder_with_cels(paint, 1, 1);
    paint.command("anim.layer.camera");
    paint.settle(2);
    let camera = named(paint, "Camera 1");
    let texts = paint.settle(2);
    let row = texts
        .iter()
        .filter(|(text, rect)| text == "Camera 1" && rect.left() > 1150.0)
        .map(|(_, rect)| *rect)
        .next()
        .expect("Camera 1's row in the Layer palette");
    click_at(paint, row.center());
    paint.step(vec![key(egui::Key::O, egui::Modifiers::NONE)]);
    paint.settle(4);
    paint.click_text("Tool Property");
    paint.settle(4);
    camera
}

/// The canvas size of the open picture.
fn canvas_size(paint: &PaintDriver) -> [u32; 2] {
    arena(paint)
        .get_by_id(open_picture(paint))
        .ok()
        .and_then(sue_model::picture::size)
        .expect("a canvas size")
}

// TRV-11 (and ABV-20): the camera's Tool Property.

#[test]
fn trv_11_camera_tool_property_keys_each_value_with_its_own_button() {
    let mut paint = PaintDriver::new("p6-camera-values");
    let camera = camera_picked(&mut paint);
    // Every value of the 2D camera, in Clip Studio Paint's order, each with
    // a key button of its own on its field's row.
    let names = [
        "Position X",
        "Position Y",
        "Scale",
        "Rotation",
        "Center of Rotation X",
        "Center of Rotation Y",
        "Layer Opacity",
    ];
    for (name, channel) in names.into_iter().zip(sue_paint::camera::VALUES) {
        let field = field_under(&mut paint, name);
        let at = mark(&paint, &format!("mark.camera.key.{channel}"))
            .unwrap_or_else(|| panic!("the key button of {channel}"));
        assert!(
            (at.y - field.center().y).abs() < 14.0 && at.x > field.right(),
            "{channel}'s key button beside its field: {at:?} {field:?}"
        );
    }
    let size = canvas_size(&paint);
    let rate = timeline(&paint).rate;
    // At frame 5, the opacity typed: that value alone is keyed there, as
    // one undo step.
    for _ in 0..4 {
        paint.command("anim.next");
    }
    paint.settle(4);
    let at5 = sue_paint::camera::time_of(4, rate);
    undo_pair(&mut paint, "typing the camera's opacity", |paint| {
        let field = field_under(paint, "Layer Opacity");
        type_enter(paint, field, "40");
    });
    let a = arena(&paint);
    assert!(sue_paint::camera::keyed(
        a,
        camera,
        sue_paint::camera::OPACITY,
        at5
    ));
    assert!(
        !sue_paint::camera::keyed(a, camera, sue_paint::camera::X, at5),
        "the position keeps no key"
    );
    let opacity = sue_paint::camera::Camera::opacity(a, camera, at5).unwrap();
    assert!((opacity - 0.4).abs() < 1e-4, "{opacity}");
    // The centre of rotation's key button keys it where it stands (the
    // middle of the view), and a second press takes that key away.
    in_view(&mut paint, "Center of Rotation X");
    let button = mark(&paint, "mark.camera.key.camera.pivot.x").unwrap();
    undo_pair(&mut paint, "keying the centre of rotation", |paint| {
        click_at(paint, button);
    });
    let a = arena(&paint);
    assert!(sue_paint::camera::keyed(
        a,
        camera,
        sue_paint::camera::PIVOT_X,
        at5
    ));
    let values = sue_paint::camera::values(a, camera, Some(at5), size);
    assert!(
        (values[4] - f64::from(size[0]) / 2.0).abs() < 1e-3,
        "{values:?}"
    );
    let button = mark(&paint, "mark.camera.key.camera.pivot.x").unwrap();
    click_at(&mut paint, button);
    assert!(!sue_paint::camera::keyed(
        arena(&paint),
        camera,
        sue_paint::camera::PIVOT_X,
        at5
    ));
    // The centre of rotation put at the canvas's corner and the camera
    // turned a quarter: its view swings round that corner.
    paint.settle(2);
    for (name, value) in [
        ("Center of Rotation X", "0"),
        ("Center of Rotation Y", "0"),
        ("Rotation", "90"),
    ] {
        let field = field_under(&mut paint, name);
        type_enter(&mut paint, field, value);
        paint.settle(2);
    }
    let a = arena(&paint);
    for channel in [
        sue_paint::camera::PIVOT_X,
        sue_paint::camera::PIVOT_Y,
        sue_paint::camera::ANGLE,
    ] {
        assert!(
            sue_paint::camera::keyed(a, camera, channel, at5),
            "{channel}"
        );
    }
    let turned = sue_paint::camera::Camera::of(a, camera, Some(at5), size);
    let (w, h) = (size[0] as f32 / 2.0, size[1] as f32 / 2.0);
    assert!(
        (turned.center[0] + h).abs() < 0.5 && (turned.center[1] - w).abs() < 0.5,
        "the middle of the view swung a quarter round the corner: {turned:?}"
    );
    // The Tool Property reads what was keyed.
    let field = field_under(&mut paint, "Rotation");
    let shown: String = paint
        .texts
        .iter()
        .find(|(_, rect)| *rect == field)
        .map(|(text, _)| text.clone())
        .unwrap();
    assert!(shown.starts_with("90"), "{shown}");
    // The frame on the canvas is drawn where the camera stands now.
    assert!(mark(&paint, "mark.camera.corner.3").is_some());
    assert!(shown_at(&paint, 0).is_some(), "the cel still shows");
}

// TRV-6 (and ABV-3): the ruler goes on left of frame 1.

/// The numbers the Timeline's ruler drew, with where.
fn ruler_numbers(paint: &mut PaintDriver) -> Vec<(i64, egui::Rect)> {
    let texts = paint.settle(4);
    let header = texts
        .iter()
        .find(|(text, rect)| text == "Timeline" && rect.top() > 600.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the Timeline palette: {texts:?}"));
    // The ruler's frame numbers sit in its lower half, the first line of
    // text under the palette's tab that holds numbers only.
    let mut numbers: Vec<(i64, egui::Rect)> = texts
        .iter()
        .filter(|(_, rect)| rect.left() > 560.0 && rect.top() > header.bottom())
        .filter_map(|(text, rect)| Some((text.parse::<i64>().ok()?, *rect)))
        .collect();
    let row = numbers
        .iter()
        .filter(|(number, _)| *number == 1)
        .map(|(_, rect)| rect.center().y)
        .fold(f32::INFINITY, f32::min);
    numbers.retain(|(_, rect)| (rect.center().y - row).abs() < 3.0);
    numbers.sort_by(|a, b| a.1.left().total_cmp(&b.1.left()));
    // The cel picture's own "1" can stand on the same line: each number
    // once, where the ruler drew it first.
    let mut seen = std::collections::HashSet::new();
    numbers.retain(|(number, _)| seen.insert(*number));
    numbers
}

#[test]
fn trv_6_the_ruler_goes_on_left_of_frame_1_numbered_and_shaded() {
    let mut paint = PaintDriver::new("p6-ruler-before");
    paint.new_timeline();
    folder_with_cels(&mut paint, 1, 1);
    // Zoomed in, every frame is numbered.
    for _ in 0..3 {
        paint.command("anim.zoom_in");
    }
    paint.command("anim.first");
    let numbers = ruler_numbers(&mut paint);
    let shown: Vec<i64> = numbers.iter().map(|(number, _)| *number).collect();
    assert!(
        shown.starts_with(&[1, 2, 3, 4]),
        "every frame numbered: {shown:?}"
    );
    assert!(shown.iter().all(|number| *number > 0), "{shown:?}");
    let one = numbers[0].1;
    // Shift and the wheel over the frames pan left of frame 1: 0, -1 and
    // on come in, in order, left of 1.
    let over = Pos2::new(one.left() + 200.0, one.bottom() + 40.0);
    for _ in 0..6 {
        paint.step(vec![
            egui::Event::PointerMoved(over),
            egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 200.0),
                modifiers: egui::Modifiers::SHIFT,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
    }
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    paint.settle(30);
    let numbers = ruler_numbers(&mut paint);
    let shown: Vec<i64> = numbers.iter().map(|(number, _)| *number).collect();
    let at = |wanted: i64| {
        numbers
            .iter()
            .find(|(number, _)| *number == wanted)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("{wanted} on the ruler: {shown:?}"))
    };
    let (minus_one, zero, one) = (at(-1), at(0), at(1));
    assert!(
        minus_one.left() < zero.left() && zero.left() < one.left(),
        "{shown:?}"
    );
    // Never more than a third of what shows lies before frame 1.
    let before = shown.iter().filter(|number| **number <= 0).count();
    assert!(
        before * 3 <= shown.len() + 3,
        "frame 1 keeps most of the width: {shown:?}"
    );
    // A click on the ruler there goes to the first frame.
    paint.command("anim.next");
    paint.command("anim.next");
    paint.settle(2);
    click_at(&mut paint, zero.center());
    let frame = sue_paint_frame(&paint);
    assert_eq!(frame, 0, "the first frame");
    // And the playback range still starts at the first frame.
    let picture = open_picture(&paint);
    let (start, _) =
        sue_paint::anim::setup::Settings::of(arena(&paint), picture).range(timeline_length(&paint));
    assert_eq!(start, 0);
}

/// The frame Sue Paint shows (0 based), as the Timeline bar reads it.
fn sue_paint_frame(paint: &PaintDriver) -> u32 {
    paint
        .texts
        .iter()
        .find(|(text, _)| text.contains(" to ") && text.contains("   "))
        .and_then(|(text, _)| text.split_whitespace().next()?.parse::<u32>().ok())
        .map(|number| number - 1)
        .unwrap_or_else(|| panic!("the bar's readout: {:?}", paint.texts))
}

/// How many frames the timeline has.
fn timeline_length(paint: &PaintDriver) -> u32 {
    sue_paint::anim::length(arena(paint), open_picture(paint))
}

// TRV-8 (and A25V-18): the rest of Animation, Edit Track.

/// The labels of the Animation menu's submenu `title`, in order.
fn animation_submenu(title: &str) -> Vec<String> {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let menus = sue_paint::menus();
    let animation = menus
        .iter()
        .find(|menu| menu.title == "Animation")
        .expect("an Animation menu");
    let mut out = Vec::new();
    for item in &animation.items {
        if let sue_wm::MenuItem::Sub { label, items } = item
            && label == title
        {
            out.extend(items.iter().map(|item| item.label().to_owned()));
        }
    }
    out
}

/// The name of the cel the first animation folder shows at `frame`.
fn shown_name(paint: &PaintDriver, frame: u32) -> Option<String> {
    shown_at(paint, frame).map(|cel| arena(paint).get_by_id(cel).unwrap().name().to_owned())
}

#[test]
fn trv_8_edit_track_has_the_entries_of_clip_studio_paint_in_its_order() {
    let edit_track = animation_submenu("Edit Track");
    let at = |name: &str| {
        edit_track
            .iter()
            .position(|label| label == name)
            .unwrap_or_else(|| panic!("{name} in Edit Track: {edit_track:?}"))
    };
    // The keyframe entries: the three eases, Edit Layers with Active
    // Keyframes, Delete All Keyframes; then the cel entries.
    assert!(at("Edit Layers with Active Keyframes") < at("Delete All Keyframes"));
    assert!(at("Edit Layers with Active Keyframes") > at("Enable Keyframes on This Layer"));
    assert_eq!(
        at("Assign Multiple Cels..."),
        at("Assign Animation Cel") + 1
    );
    assert_eq!(
        at("Create All Supported Cels"),
        at("Assign Multiple Cels...") + 1
    );
    assert!(at("Create All Supported Cels") < at("Rename in Timeline Order"));
}

#[test]
fn trv_8_assign_multiple_cels_then_create_all_supported_cels() {
    let mut paint = PaintDriver::new("p6-assign-many");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 4);
    paint.command("anim.first");
    paint.settle(2);
    // Assign Multiple Cels from frame 1: 1, 2, then 3 and 4 (not drawn
    // yet), on twos.
    paint.command("anim.cel.assign_many");
    let texts = paint.settle(4);
    assert!(
        texts.iter().any(|(text, _)| text == "Assign Multiple Cels"),
        "the dialog: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|(text, _)| text.contains("Cels of Folder 1: 1, 2")),
        "it lists the folder's cels: {texts:?}"
    );
    super::paint_anim::fill(&mut paint, "Cels", "1, 2, 3-4");
    super::paint_anim::fill(&mut paint, "Frames each", "2");
    undo_pair(&mut paint, "Assign Multiple Cels", |paint| {
        paint.click_text("OK");
    });
    let shown: Vec<Option<String>> = (0..8).map(|frame| shown_name(&paint, frame)).collect();
    assert_eq!(
        shown,
        [
            Some("1"),
            Some("1"),
            Some("2"),
            Some("2"),
            None,
            None,
            None,
            None
        ]
        .map(|name| name.map(str::to_owned))
        .to_vec(),
        "on twos; 3 and 4 show nothing yet"
    );
    let sequence = timeline(&paint);
    assert_eq!(
        sue_paint::anim::editing::wanted_cels(
            &sequence.tracks[folder_track(&paint)],
            sequence.rate
        ),
        vec![(4, "3".to_owned()), (6, "4".to_owned())]
    );
    // The timeline names them on their frames, before they exist.
    let texts = paint.settle(2);
    let header = texts
        .iter()
        .find(|(text, rect)| text == "Timeline" && rect.top() > 600.0)
        .map(|(_, rect)| *rect)
        .unwrap();
    for name in ["3", "4"] {
        assert!(
            texts.iter().any(|(text, rect)| text == name
                && rect.left() > 560.0
                && rect.top() > header.bottom() + 40.0),
            "{name} on the folder's track: {texts:?}"
        );
    }
    // Create All Supported Cels makes 3 and 4, shown where they were
    // named.
    undo_pair(&mut paint, "Create All Supported Cels", |paint| {
        paint.command("anim.cel.create_supported");
    });
    let shown: Vec<Option<String>> = (0..8).map(|frame| shown_name(&paint, frame)).collect();
    assert_eq!(
        shown,
        ["1", "1", "2", "2", "3", "3", "4", "4"]
            .map(|name| Some(name.to_owned()))
            .to_vec()
    );
    let folder =
        sue_paint::anim::track_folder(&timeline(&paint).tracks[folder_track(&paint)]).unwrap();
    assert_eq!(
        sue_paint::anim::cels(arena(&paint), open_picture(&paint), folder).len(),
        4,
        "two cels made, in the folder"
    );
    // Nothing left to make: no step.
    let steps = paint.runtime.engine.history.depth();
    paint.command("anim.cel.create_supported");
    paint.settle(2);
    assert_eq!(paint.runtime.engine.history.depth(), steps);
}

/// The columns of `layer` that hold ink in the rows `rows`: the first
/// and the last, if any.
fn ink_columns(
    paint: &PaintDriver,
    layer: sue_core::EntityId,
    rows: std::ops::RangeInclusive<u32>,
) -> Option<(u32, u32)> {
    let raster = sue_paint::doc::raster(arena(paint), layer).unwrap();
    let bytes = raster.bytes().unwrap();
    let width = raster.width();
    let inked: Vec<u32> = (0..width)
        .filter(|x| {
            rows.clone()
                .filter(|y| *y < raster.height())
                .any(|y| bytes[((y * width + x) * 4 + 3) as usize] > 64)
        })
        .collect();
    Some((*inked.first()?, *inked.last()?))
}

/// The rows of `layer` that hold ink: the first and the last.
fn ink_rows(paint: &PaintDriver, layer: sue_core::EntityId) -> (u32, u32) {
    let raster = sue_paint::doc::raster(arena(paint), layer).unwrap();
    let bytes = raster.bytes().unwrap();
    let width = raster.width();
    let inked: Vec<u32> = (0..raster.height())
        .filter(|y| (0..width).any(|x| bytes[((y * width + x) * 4 + 3) as usize] > 64))
        .collect();
    (inked[0], *inked.last().unwrap())
}

#[test]
fn trv_8_edit_layers_with_active_keyframes_draws_where_the_layer_shows() {
    let mut paint = PaintDriver::new("p6-edit-keyed");
    paint.new_timeline();
    let middle = paint_middle(&paint.runtime);
    // A filled box on Layer 1, keyed, then moved right with the Object
    // tool at frame 1.
    paint.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    paint.drag(
        middle - vec2(40.0, 20.0),
        middle + vec2(40.0, 20.0),
        egui::PointerButton::Primary,
    );
    paint.command("edit.fill");
    paint.command("select.none");
    paint.settle(4);
    let layer = named(&paint, "Layer 1");
    paint.command("anim.keyframes");
    paint.command("anim.key.layer");
    paint.command("tool.operation");
    paint.settle(4);
    let inside =
        mark(&paint, "mark.keyed.middle").expect("the keyed layer's box") + vec2(10.0, 5.0);
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::SHIFT,
    };
    paint.step(vec![
        egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
        egui::Event::PointerMoved(inside),
    ]);
    paint.step(vec![event(inside, true)]);
    for index in 1..=16 {
        paint.step(vec![egui::Event::PointerMoved(
            inside + vec2(120.0 * index as f32 / 16.0, 0.0),
        )]);
    }
    paint.step(vec![event(inside + vec2(120.0, 0.0), false)]);
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    paint.settle(6);
    let size = canvas_size(&paint);
    let rate = timeline(&paint).rate;
    let pose = sue_paint::anim::motion::pose(
        arena(&paint),
        layer,
        sue_paint::camera::time_of(0, rate),
        size,
    )
    .unwrap();
    let dx = pose.position[0];
    assert!(dx > 40.0, "the layer moved right: {pose:?}");
    // A reference line on a layer of its own, above the box.
    paint.command("layer.new");
    paint.settle(2);
    let reference = sue_paint::doc::layers(arena(&paint), open_picture(&paint))
        .last()
        .unwrap()
        .id;
    assert!(reference != layer);
    paint.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    paint.settle(2);
    line_on_canvas(&mut paint, -100.0);
    let rows = ink_rows(&paint, reference);
    let (left, right) = ink_columns(&paint, reference, rows.0..=rows.1).unwrap();
    paint.command("edit.undo");
    paint.command("edit.undo");
    paint.settle(2);
    // Back on Layer 1: with keyframes on it takes no stroke.
    assert_eq!(
        sue_paint::doc::layers(arena(&paint), open_picture(&paint))
            .last()
            .unwrap()
            .id,
        layer
    );
    let before = arena_bytes(&paint);
    line_on_canvas(&mut paint, -100.0);
    assert!(arena_bytes(&paint) == before, "refused");
    // Edit Layers with Active Keyframes: the same line lands on the layer
    // where it shows, so in the layer's own pixels it is left of where it
    // shows by as much as the layer moved.
    paint.command("anim.key.edit_keyed");
    paint.settle(2);
    undo_pair(&mut paint, "a stroke on a keyed layer", |paint| {
        line_on_canvas(paint, -100.0);
    });
    let (keyed_left, keyed_right) =
        ink_columns(&paint, layer, rows.0..=rows.1).expect("ink on the keyed layer");
    let shift = |a: u32, b: u32| (a as f32 - b as f32 - dx).abs();
    assert!(
        shift(left, keyed_left) < 3.0 && shift(right, keyed_right) < 3.0,
        "drawn where it shows: {left}..{right} against {keyed_left}..{keyed_right}, moved {dx}"
    );
    // And the canvas shows it there, under the pen, while it is drawn
    // and once it is in: the middle of the line, on its row.
    let row = (rows.0 + rows.1) / 2;
    let under = [(left + right) / 2, row];
    let dark = |paint: &mut PaintDriver| {
        paint.settle(2);
        let seen = sue_paint::canvas_probed();
        seen.first()
            .copied()
            .flatten()
            .is_some_and(|[r, g, b, _]| u32::from(r) + u32::from(g) + u32::from(b) < 300)
    };
    sue_paint::probe_canvas(vec![under]);
    assert!(dark(&mut paint), "the stroke shows under the pen");
    paint.command("edit.undo");
    paint.settle(4);
    assert!(!dark(&mut paint), "undone: nothing under the pen");
    let middle = paint_middle(&paint.runtime);
    let (from, to) = (middle + vec2(-80.0, -100.0), middle + vec2(80.0, -100.0));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(from)]);
    paint.step(vec![button(from, true)]);
    for index in 1..=24 {
        paint.step(vec![egui::Event::PointerMoved(
            from + (to - from) * (index as f32 / 24.0),
        )]);
    }
    assert!(dark(&mut paint), "shown under the pen while it is drawn");
    paint.step(vec![button(to, false)]);
    paint.settle(4);
    assert!(dark(&mut paint), "shown under the pen once it is in");
    sue_paint::probe_canvas(Vec::new());
    // Off again: refused again.
    paint.command("anim.key.edit_keyed");
    let before = arena_bytes(&paint);
    line_on_canvas(&mut paint, -100.0);
    assert!(arena_bytes(&paint) == before, "refused again");
}

// Seen and left in Phase 5: in-between marks in the label strip; Range
// off grays the frame count.

#[test]
fn trv_6_label_strip_holds_the_in_between_marks_over_the_cels() {
    let mut paint = PaintDriver::new("p6-inbetween-strip");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 4);
    paint.command("anim.next");
    paint.command("anim.next");
    paint.command("anim.track_label.inbetween");
    paint.command("anim.next");
    paint.command("anim.track_label.inbetween-done");
    let texts = paint.settle(4);
    let track = folder_track(&paint);
    let marks: Vec<Pos2> = [2, 3]
        .iter()
        .map(|frame| {
            mark(&paint, &format!("mark.track-label.{track}.{frame}"))
                .unwrap_or_else(|| panic!("the mark at frame {}", frame + 1))
        })
        .collect();
    // In one strip at the top of the folder's row: above the cels' names
    // (cel 1 named on its clip), at the same height.
    let cel_name = texts
        .iter()
        .filter(|(text, rect)| text == "1" && rect.left() > 560.0 && rect.top() > 780.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("cel 1 on its track: {texts:?}"));
    for at in &marks {
        assert!(
            at.y < cel_name.top(),
            "the mark over the cels: {at:?} {cel_name:?}"
        );
    }
    assert!((marks[0].y - marks[1].y).abs() < 0.5, "one strip");
}

#[test]
fn trv_6_track_label_range_off_grays_the_frame_count() {
    let mut paint = PaintDriver::new("p6-range-gray");
    paint.new_timeline();
    folder_with_cels(&mut paint, 1, 1);
    paint.command("anim.track_note");
    paint.settle(4);
    assert!(
        paint.rect_of("Number of frames").is_some(),
        "the frame count stays with Range off: {:?}",
        paint.texts
    );
    // Off, it is grayed: a drag along it (which changes a number field)
    // leaves it at 1.
    let field = super::paint_anim::right_of(&paint, "Number of frames");
    let at = field.center();
    paint.drag(at, at + vec2(80.0, 0.0), egui::PointerButton::Primary);
    paint.settle(2);
    // (A press there may move the dialog: it is read where it is now.)
    let field = super::paint_anim::right_of(&paint, "Number of frames");
    let shown = paint
        .texts
        .iter()
        .find(|(_, rect)| *rect == field)
        .map(|(text, _)| text.clone());
    assert_eq!(shown.as_deref(), Some("1"), "grayed, it keeps 1");
    // Range ticked: the same field takes the count.
    paint.click_text("Range");
    paint.settle(2);
    super::paint_anim::fill(&mut paint, "Number of frames", "4");
    super::paint_anim::fill(&mut paint, "Label name", "Walk");
    paint.click_text("OK");
    paint.settle(2);
    let sequence = timeline(&paint);
    let notes = sue_paint::anim::editing::track_notes(&sequence.tracks[folder_track(&paint)]);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!((notes[0].name.as_str(), notes[0].range), ("Walk", 4));
}

#[test]
fn trv_8_a_stroke_under_a_rendered_2d_camera_lands_where_the_cel_shows() {
    let mut paint = PaintDriver::new("p6-stroke-camera");
    // Cel 1 with a line, drawn before any camera: where it lands is where
    // the pen went. Then its folder dragged into a new camera folder.
    paint.new_timeline();
    folder_with_cels(&mut paint, 1, 1);
    paint.command("anim.layer.camera");
    let camera = named(&paint, "Camera 1");
    let folder = named(&paint, "Folder 1");
    let row = |paint: &mut PaintDriver, name: &str| {
        let texts = paint.settle(2);
        texts
            .iter()
            .filter(|(text, rect)| text.starts_with(name) && rect.left() > 1150.0)
            .map(|(_, rect)| *rect)
            .next()
            .unwrap_or_else(|| panic!("{name}'s row in the Layer palette: {texts:?}"))
    };
    let (from, onto) = (row(&mut paint, "Folder 1"), row(&mut paint, "Camera 1"));
    paint.drag(from.center(), onto.center(), egui::PointerButton::Primary);
    paint.settle(2);
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), folder),
        Some(camera),
        "the folder in the camera"
    );
    let at = row(&mut paint, "Camera 1");
    click_at(&mut paint, at.center());
    paint.step(vec![key(egui::Key::O, egui::Modifiers::NONE)]);
    paint.settle(4);
    paint.click_text("Tool Property");
    paint.settle(4);
    let cel = named(&paint, "1");
    let first = ink_rows(&paint, cel);
    let (left, right) = ink_columns(&paint, cel, first.0..=first.1).expect("cel 1's line");
    // The camera moved 100 pixels right at frame 1; Render 2D Camera is
    // on (as it is by default), so the canvas shows the cels through it.
    let size = canvas_size(&paint);
    let field = field_under(&mut paint, "Position X");
    type_enter(
        &mut paint,
        field,
        &format!("{}", f64::from(size[0]) / 2.0 + 100.0),
    );
    let rate = timeline(&paint).rate;
    let seen = sue_paint::camera::Camera::of(
        arena(&paint),
        camera,
        Some(sue_paint::camera::time_of(0, rate)),
        size,
    );
    assert!(
        (seen.center[0] - (size[0] as f32 / 2.0 + 100.0)).abs() < 0.5,
        "{seen:?}"
    );
    assert!(
        sue_paint::anim::setup::Playback::of(arena(&paint), open_picture(&paint)).render_camera
    );
    // Back on cel 1 with the pen, the same line lower down: it lands on
    // the cel where the camera shows it, 100 pixels right in the cel's own
    // pixels.
    paint.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    let texts = paint.settle(4);
    let at = texts
        .iter()
        .filter(|(text, rect)| text == "1" && rect.left() > 1150.0)
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("cel 1's row in the Layer palette: {texts:?}"));
    click_at(&mut paint, at.center());
    paint.settle(2);
    undo_pair(&mut paint, "a stroke through the camera", |paint| {
        line_on_canvas(paint, 60.0);
    });
    let last = ink_rows(&paint, cel);
    assert!(
        last.1 > first.1 + 4,
        "a second line, lower: {first:?} {last:?}"
    );
    let (moved_left, moved_right) =
        ink_columns(&paint, cel, first.1 + 4..=last.1).expect("the second line");
    let shift = |a: u32, b: u32| (b as f32 - a as f32 - 100.0).abs();
    assert!(
        shift(left, moved_left) < 3.0 && shift(right, moved_right) < 3.0,
        "drawn where it shows: {left}..{right} against {moved_left}..{moved_right}"
    );
    // The canvas shows it under the pen, as it was drawn.
    let inked: Vec<u32> = (first.1 + 4..=last.1)
        .filter(|y| {
            let raster = sue_paint::doc::raster(arena(&paint), cel).unwrap();
            let x = (moved_left + moved_right) / 2;
            raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize] > 64
        })
        .collect();
    let row = inked[inked.len() / 2];
    sue_paint::probe_canvas(vec![[(left + right) / 2, row]]);
    paint.settle(4);
    let seen = sue_paint::canvas_probed();
    sue_paint::probe_canvas(Vec::new());
    assert!(
        seen.first()
            .copied()
            .flatten()
            .is_some_and(|[r, g, b, _]| u32::from(r) + u32::from(g) + u32::from(b) < 300),
        "shown under the pen: {seen:?}"
    );
}
