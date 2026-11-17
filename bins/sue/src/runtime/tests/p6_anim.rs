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
