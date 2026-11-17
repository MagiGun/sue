//! Sue Paint 0.0.81: the 3D drawing figures driven through real frames.
//! A figure is added, made another model and shaped part by part; a hand
//! dragged on the canvas takes its arm along; a ring of the rotation
//! manipulator turns one joint; a hand pose dragged from the Material
//! palette closes the figure's hands; a glTF and an OBJ file are imported
//! and the glTF posed by its bone. Each is one undo step that undoes to
//! the arena exactly as it was and redoes exactly.

use super::paint_3d::{
    arena_bytes, click_control, drag_control, model, new_3d, one_step, picture, pixels, step_told,
};
use super::*;

use sue_paint::figure::{Body, Kind, Pose, Side};

fn layers(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(&paint.runtime.engine.world.arena, picture(paint))
}

/// The zoom, as the window's title carries it.
fn zoom(paint: &PaintDriver) -> f32 {
    let title = paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(&paint.runtime.engine.world.arena))
        .expect("Sue Paint names its window");
    title
        .split_whitespace()
        .find_map(|word| {
            word.trim_end_matches(')')
                .strip_suffix('%')
                .and_then(|number| number.parse::<f32>().ok())
        })
        .expect("the title carries the zoom")
        / 100.0
}

/// Where picture point `p` is on screen while the view has not moved.
fn screen_of(paint: &PaintDriver, p: [f32; 2]) -> Pos2 {
    let [w, h] = sue_model::picture::size(
        paint
            .runtime
            .engine
            .world
            .arena
            .get_by_id(picture(paint))
            .unwrap(),
    )
    .unwrap();
    paint_middle(&paint.runtime) + vec2(p[0] - w as f32 / 2.0, p[1] - h as f32 / 2.0) * zoom(paint)
}

/// The figure of `layer`: its pose, its body and the layer's camera.
fn sight(paint: &PaintDriver, layer: sue_core::EntityId) -> (Pose, Body, sue_paint::three::Camera) {
    let arena = &paint.runtime.engine.world.arena;
    let entity = model(paint, layer);
    let (pose, body) = (Pose::of(entity), Body::of(entity));
    let raster = sue_paint::doc::raster(arena, layer).unwrap();
    let view = sue_paint::three::View3D::of(arena.get_by_id(layer).unwrap());
    let lens =
        sue_paint::three::Camera::new(&view, [raster.width(), raster.height()], body.frame());
    (pose, body, lens)
}

/// Where the model point `p` of the figure of `layer` is on screen.
fn on_screen(paint: &PaintDriver, layer: sue_core::EntityId, p: [f32; 3]) -> Pos2 {
    let (_, _, lens) = sight(paint, layer);
    screen_of(paint, lens.project(p).expect("in front of the camera"))
}

/// The layer added since `before`.
fn added(paint: &PaintDriver, before: &[sue_paint::doc::LayerInfo]) -> sue_core::EntityId {
    layers(paint)
        .into_iter()
        .find(|info| !before.iter().any(|old| old.id == info.id))
        .expect("a new layer")
        .id
}

fn undo(paint: &mut PaintDriver) {
    paint.command("edit.undo");
    paint.settle(4);
}

fn redo(paint: &mut PaintDriver) {
    paint.command("edit.redo");
    paint.settle(4);
}

/// Does `change`, which must change the document so that `check` holds,
/// as one undo step; undo gives back the arena as before, redo as after.
fn undo_pair(
    paint: &mut PaintDriver,
    what: &str,
    change: impl FnOnce(&mut PaintDriver),
    check: impl Fn(&PaintDriver) -> bool,
) {
    paint.settle(2);
    let before = arena_bytes(paint);
    let depth = paint.runtime.engine.history.depth();
    assert!(!check(paint), "{what}: not so before");
    change(paint);
    paint.settle(4);
    assert!(
        check(paint),
        "{what}: so after ({:?}, {} layers)",
        paint
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_string()),
        layers(paint).len()
    );
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "{what}: one undo step"
    );
    let after = arena_bytes(paint);
    undo(paint);
    assert!(arena_bytes(paint) == before, "{what}: undone exactly");
    redo(paint);
    assert!(arena_bytes(paint) == after, "{what}: redone exactly");
}

#[test]
fn p81_a_figure_is_added_made_another_model_and_shaped_part_by_part() {
    let mut paint = PaintDriver::new("p81-figure-shape");
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "a new figure",
        |paint| {
            paint.command("layer.new.figure");
            paint.settle(4);
        },
        |paint| layers(paint).len() == count + 1,
    );
    // One figure from here on.
    undo(&mut paint);
    let layer = new_3d(&mut paint, "layer.new.figure");
    let body = |paint: &PaintDriver| Body::of(model(paint, layer));
    assert_eq!(body(&paint).kind, Kind::Body);
    assert!(body(&paint).ground, "a new figure keeps to the ground");
    // The body shape by part: the waist slider, dragged.
    one_step(
        &mut paint,
        layer,
        "a wider waist",
        |paint| drag_control(paint, "Waist", vec2(40.0, 0.0)),
        |paint| assert!(body(paint).slider("waist") > 0.1, "{:?}", body(paint).parts),
    );
    one_step(
        &mut paint,
        layer,
        "longer legs",
        |paint| drag_control(paint, "Leg length", vec2(-50.0, 0.0)),
        |paint| assert!(body(paint).slider("leg_length") < -0.1),
    );
    // The hands' veins: a slider of the body shape too.
    one_step(
        &mut paint,
        layer,
        "more veins",
        |paint| drag_control(paint, "Veins", vec2(60.0, 0.0)),
        |paint| assert!(body(paint).slider("veins") > 0.5),
    );
    one_step(
        &mut paint,
        layer,
        "no guide lines",
        |paint| click_control(paint, "Show guide lines"),
        |paint| assert!(!body(paint).guides),
    );
    // Shapes are kept with the model as properties.
    let kept = model(&paint, layer)
        .property(sue_paint::figure::PARTS_TEXT)
        .and_then(sue_core::Value::as_str)
        .unwrap()
        .to_owned();
    assert!(kept.contains("waist=") && kept.contains("veins="), "{kept}");
    // The model chooser's commands: a female body keeps the pose and
    // the shape; a right hand starts in the hand's first pose.
    one_step(
        &mut paint,
        layer,
        "the female body",
        |paint| {
            paint.command("figure.model.1");
            paint.settle(4);
        },
        |paint| assert_eq!(body(paint).build, sue_paint::figure::Build::Female),
    );
    assert!(body(&paint).slider("waist") > 0.1, "the shape stays");
    one_step(
        &mut paint,
        layer,
        "a right hand",
        |paint| {
            paint.command("figure.model.3");
            paint.settle(4);
        },
        |paint| {
            let body = body(paint);
            assert_eq!((body.kind, body.side), (Kind::Hand, Side::Right));
            assert_eq!(Pose::of(model(paint, layer)).0.len(), 16);
        },
    );
    // The hand has its own rows: the hand setup's spread slider spreads
    // the fingers.
    one_step(
        &mut paint,
        layer,
        "fingers spread",
        |paint| drag_control(paint, "Finger spread", vec2(60.0, 0.0)),
        |paint| {
            let pose = Pose::of(model(paint, layer));
            assert!(pose.0[1][2] > 3.0 && pose.0[10][2] < -3.0, "{:?}", pose.0);
        },
    );
}

#[test]
fn p81_a_dragged_hand_takes_its_arm_along_and_a_ring_turns_one_joint() {
    let mut paint = PaintDriver::new("p81-figure-ik");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let (base, body, _) = sight(&paint, layer);
    let wrist = sue_paint::figure::solve(&base, &body)[7].0;
    let from = on_screen(&paint, layer, wrist);
    let to = from + vec2(-36.0, -70.0);
    one_step(
        &mut paint,
        layer,
        "the left hand dragged up",
        |paint| {
            paint.drag(from, to, egui::PointerButton::Primary);
        },
        |paint| {
            let (pose, body, _) = sight(paint, layer);
            assert_ne!(pose.0[5], base.0[5], "the upper arm turned");
            assert!(pose.0[6][0] < base.0[6][0] - 10.0, "the elbow folded");
            assert!(
                pose.0[6][0] >= -155.0 && pose.0[6][2].abs() < 1e-3,
                "within limits"
            );
            // Nothing but the arm moved.
            for joint in (0..17).filter(|joint| *joint != 5 && *joint != 6) {
                assert_eq!(pose.0[joint], base.0[joint], "{joint}");
            }
            // The wrist is where it was let go.
            let now = on_screen(paint, layer, sue_paint::figure::solve(&pose, &body)[7].0);
            assert!((now - to).length() < 14.0, "{now:?} for {to:?}");
        },
    );
    // The dragged part is picked, so its rotation rings show; the first
    // ring (the joint's own x) turned by a drag along it.
    let (pose, body, _) = sight(&paint, layer);
    let placed = sue_paint::figure::solve(&pose, &body);
    let picked = (5..=7)
        .find(|joint| {
            sue_paint::figure::rings(&pose, &body, *joint).is_some_and(|rings| {
                let _ = &placed;
                !rings[0].is_empty()
            })
        })
        .unwrap();
    // Which joint the drag picked: the one whose ring a press finds.
    let mut turned = false;
    for joint in [6, 7, picked] {
        let Some(rings) = sue_paint::figure::rings(&pose, &body, joint) else {
            continue;
        };
        let (a, b) = (
            on_screen(&paint, layer, rings[0][6]),
            on_screen(&paint, layer, rings[0][14]),
        );
        let before = arena_bytes(&paint);
        let depth = paint.runtime.engine.history.depth();
        paint.drag(a, b, egui::PointerButton::Primary);
        paint.settle(4);
        let (after, _, _) = sight(&paint, layer);
        if after.0[joint] != pose.0[joint] {
            assert_eq!(paint.runtime.engine.history.depth(), depth + 1, "one step");
            for other in (0..17).filter(|other| *other != joint) {
                assert_eq!(after.0[other], pose.0[other], "only joint {joint} turned");
            }
            let kept = arena_bytes(&paint);
            undo(&mut paint);
            assert!(
                arena_bytes(&paint) == before,
                "the ring turn undone exactly"
            );
            redo(&mut paint);
            assert!(arena_bytes(&paint) == kept, "and redone exactly");
            turned = true;
            break;
        }
        // Not the picked joint: whatever the drag did instead is undone.
        while paint.runtime.engine.history.depth() > depth {
            undo(&mut paint);
        }
    }
    assert!(turned, "a ring of the picked joint turned it");
    // Flip pose mirrors it: the right arm takes the left arm's fold.
    let (posed, _, _) = sight(&paint, layer);
    one_step(
        &mut paint,
        layer,
        "flip pose",
        |paint| {
            paint.command("figure.flip");
            paint.settle(4);
        },
        |paint| {
            let (flipped, _, _) = sight(paint, layer);
            assert_eq!(flipped.0[9][0], posed.0[6][0]);
        },
    );
}

/// The control named `name` for assistive technology, anywhere.
fn named(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    let told = step_told(paint, Vec::new());
    told.iter()
        .find(|(label, ..)| label == name)
        .map(|(_, _, rect)| *rect)
        .unwrap_or_else(|| {
            panic!(
                "{name:?} is named: {:?}",
                told.iter().map(|(label, ..)| label).collect::<Vec<_>>()
            )
        })
}

fn click_at(paint: &mut PaintDriver, at: Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(4);
}

/// The Material palette at `folder`: the thumbnails it shows, by name.
fn material_folder(paint: &mut PaintDriver, folder: &str) -> Vec<(String, egui::Rect)> {
    paint.settle(2);
    if paint.rect_of("Material").is_none() {
        paint.command("window.material");
        paint.settle(4);
    }
    paint.click_text("Material");
    paint.settle(4);
    let at = named(paint, folder).center();
    click_at(paint, at);
    step_told(paint, Vec::new())
        .into_iter()
        .filter(|(_, role, rect)| {
            *role == egui::accesskit::Role::Button
                && rect.width() > 40.0
                && (rect.width() - rect.height()).abs() < 2.0
        })
        .map(|(name, _, rect)| (name, rect))
        .collect()
}

/// The thumbnail named `name` of the Material palette's open folder,
/// brought near the top of the list (where `first`, the first thumbnail,
/// was before any scrolling) with the wheel.
fn scrolled_to(paint: &mut PaintDriver, name: &str, first: egui::Rect) -> egui::Rect {
    let thumbs = |paint: &mut PaintDriver| -> Vec<(String, egui::Rect)> {
        step_told(paint, Vec::new())
            .into_iter()
            .filter(|(_, role, rect)| {
                *role == egui::accesskit::Role::Button
                    && rect.width() > 40.0
                    && (rect.width() - rect.height()).abs() < 2.0
            })
            .map(|(name, _, rect)| (name, rect))
            .collect()
    };
    let over = first.center();
    for _ in 0..80 {
        let shown = thumbs(paint);
        let found = shown
            .iter()
            .find(|(shown, _)| shown == name)
            .unwrap_or_else(|| panic!("{name} among {shown:?}"))
            .1;
        if found.top() < first.top() + first.height() * 1.6 && found.top() >= first.top() - 1.0 {
            return found;
        }
        let by = if found.top() > first.top() {
            -30.0
        } else {
            30.0
        };
        paint.step(vec![
            egui::Event::PointerMoved(over),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, by),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
        paint.settle(2);
    }
    panic!("{name} never scrolled into view");
}

#[test]
fn p81_a_hand_pose_material_closes_the_hands_of_a_body() {
    let mut paint = PaintDriver::new("p81-hand-pose");
    let had = layers(&paint);
    paint.command("layer.new.figure");
    paint.settle(6);
    let layer = added(&paint, &had);
    let poses = material_folder(&mut paint, "Poses");
    let names: Vec<&str> = poses.iter().map(|(name, _)| name.as_str()).collect();
    // Sue's own library: the body poses and the hand poses.
    for name in [
        "Contrapposto",
        "Crouch",
        "Punch",
        "Look back",
        "Fist",
        "OK sign",
        "Claw",
    ] {
        assert!(names.contains(&name), "{name} among {names:?}");
    }
    assert!(names.len() >= 12 + 12, "{}", names.len());
    let first = poses
        .iter()
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("thumbnails");
    let fist = scrolled_to(&mut paint, "Fist", first);
    let (start, body, _) = sight(&paint, layer);
    let to = paint_middle(&paint.runtime);
    let layer_count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "Fist dropped onto the body",
        |paint| {
            paint.drag(fist.center(), to, egui::PointerButton::Primary);
        },
        |paint| sight(paint, layer).0 != start,
    );
    assert_eq!(layers(&paint).len(), layer_count, "no layer was added");
    let (posed, _, _) = sight(&paint, layer);
    // The body keeps its pose; both hands hold the fist.
    assert_eq!(posed.0[..17], start.0[..17]);
    let want = sue_paint::figure::hand_pose(&body, 2);
    for side in [Side::Left, Side::Right] {
        let hand = sue_paint::figure::hand_from_body(&body, &posed, side);
        for joint in 1..16 {
            for axis in 0..3 {
                assert!(
                    (hand.0[joint][axis] - want.0[joint][axis]).abs() < 0.1,
                    "{side:?} {joint}: {:?} for {:?}",
                    hand.0[joint],
                    want.0[joint]
                );
            }
        }
    }
    // A body pose dropped the same way poses the body.
    let crouch = scrolled_to(&mut paint, "Crouch", first);
    undo_pair(
        &mut paint,
        "Crouch dropped onto the body",
        |paint| {
            paint.drag(crouch.center(), to, egui::PointerButton::Primary);
        },
        |paint| sight(paint, layer).0 == body.preset(12),
    );
}

#[test]
fn p81_gltf_and_obj_files_import_as_3d_layers_and_a_skinned_one_poses() {
    let mut paint = PaintDriver::new("p81-import");
    let folder = std::env::temp_dir().join(format!("sue-p81-import-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let post = folder.join("Post.gltf");
    std::fs::write(&post, sue_paint::model3d::sample::post_gltf()).unwrap();
    let pyramid = folder.join("Pyramid.obj");
    std::fs::write(
        &pyramid,
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 2 3\nf 1 3 4\nf 1 4 2\nf 2 4 3\n",
    )
    .unwrap();
    let import = |paint: &mut PaintDriver, path: &std::path::Path| {
        paint.command("file.import.3d");
        paint.settle(2);
        let now = paint.now;
        let _ = paint.runtime.answer_pick(path, &mut NoTextures, now);
        paint.settle(6);
    };
    // The skinned glTF: a 3D layer whose model is a figure of its own
    // bones, the file kept whole inside the model.
    let count = layers(&paint).len();
    let empty = arena_bytes(&paint);
    let depth = paint.runtime.engine.history.depth();
    let had = layers(&paint);
    import(&mut paint, &post);
    assert_eq!(layers(&paint).len(), count + 1, "a glTF model imported");
    let layer = added(&paint, &had);
    assert_eq!(paint.runtime.engine.history.depth(), depth + 1, "one step");
    let entity = model(&paint, layer);
    let body = Body::of(entity);
    assert_eq!(body.kind, Kind::Imported);
    let joints = body.joints();
    assert_eq!(
        joints.iter().map(|joint| joint.name).collect::<Vec<_>>(),
        ["Root", "Arm"]
    );
    let (kept, extension) = sue_paint::model3d::file_of(entity).unwrap();
    assert_eq!(extension, "glb");
    assert!(kept.starts_with(b"glTF"));
    let bytes = arena_bytes(&paint);
    assert!(
        bytes.windows(4).any(|window| window == b"glTF"),
        "the file is in the document"
    );
    assert!(
        pixels(&paint, layer).iter().any(|byte| *byte != 0),
        "it shows"
    );
    // Its upper bone dragged to the side on the canvas: the bone turns.
    let (rest, body, _) = sight(&paint, layer);
    let placed = sue_paint::figure::bones(&rest, &body);
    let middle = [0, 1, 2].map(|axis| (placed[1].0[axis] + placed[1].1[axis]) / 2.0);
    let from = on_screen(&paint, layer, middle);
    undo_pair(
        &mut paint,
        "the imported model's bone dragged",
        |paint| {
            paint.drag(from, from + vec2(60.0, 10.0), egui::PointerButton::Primary);
        },
        |paint| sight(paint, layer).0.0[1] != rest.0[1],
    );
    // The drag and the import undo to the canvas exactly as it was, and
    // redo.
    let posed = arena_bytes(&paint);
    undo(&mut paint);
    undo(&mut paint);
    assert!(arena_bytes(&paint) == empty, "the import undone exactly");
    redo(&mut paint);
    redo(&mut paint);
    assert!(arena_bytes(&paint) == posed, "and redone exactly");
    // The OBJ: a plain model, its file kept too.
    let count = layers(&paint).len();
    let had = layers(&paint);
    undo_pair(
        &mut paint,
        "an OBJ model imported",
        |paint| import(paint, &pyramid),
        |paint| layers(paint).len() == count + 1,
    );
    let layer = added(&paint, &had);
    let entity = model(&paint, layer);
    assert!(!sue_paint::figure::is_figure(entity));
    assert_eq!(sue_paint::model3d::file_of(entity).unwrap().1, "obj");
    assert!(
        pixels(&paint, layer).iter().any(|byte| *byte != 0),
        "it shows"
    );
    // Both are kept among the 3D object materials, to use again.
    let objects = material_folder(&mut paint, "3D objects");
    let names: Vec<&str> = objects.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        names.contains(&"Post") && names.contains(&"Pyramid"),
        "{names:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
